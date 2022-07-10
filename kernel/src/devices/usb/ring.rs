extern crate alloc;
use core::marker::PhantomData;

use alloc::vec::Vec;

pub use xhci::ring::trb::command::Allowed as TrbC;
pub use xhci::ring::trb::event::Allowed as TrbE;
pub use xhci::ring::trb::transfer::Allowed as TrbT;
use xhci::ring::trb::Link;

use super::mem::XhcAlignedAllocator;

pub type EventRing = Consumer<TrbE>;
pub type TransferRing = Producer<TrbT>;
pub type CommandRing = Producer<TrbC>;

type RingAlloc = XhcAlignedAllocator<64>;
const ALLOC: RingAlloc = XhcAlignedAllocator::<64>;

pub trait SoftwareProduceTrb: Sized {
    fn link_trb() -> Self;
    fn into_raw(self) -> [u32; 4];
}
pub trait SoftwareConsumeTrb {}
fn link_trb_with_toggle() -> Link {
    let mut link = Link::new();
    link.set_toggle_cycle();
    link
}

impl SoftwareProduceTrb for TrbC {
    fn link_trb() -> Self {
        Self::Link(link_trb_with_toggle())
    }
    fn into_raw(self) -> [u32; 4] {
        self.into_raw()
    }
}
impl SoftwareConsumeTrb for TrbE {}
impl SoftwareProduceTrb for TrbT {
    fn link_trb() -> Self {
        Self::Link(link_trb_with_toggle())
    }
    fn into_raw(self) -> [u32; 4] {
        self.into_raw()
    }
}

/// Wrapper of fixed size [`Vec`] for ring management implementation.
/// Main purpose is to prevent `push` on [`Vec`] which might incur difficult bug
/// through reallocation (i.e. location changes without notifying ring related registers).
/// (Event ring can be dynamically sized, but it will be managed through [`EventRingSegmentTable`] and [`Ring`] will be inctanciated per segment.)
#[derive(Debug)]
struct Ring<Trb> {
    // Trb instead of
    buf: Vec<[u32; 4], RingAlloc>,
    _phantom: PhantomData<Trb>,
}
impl<Trb> Ring<Trb> {
    fn new(capacity: usize) -> Self {
        let mut buf = Vec::with_capacity_in(capacity, ALLOC);
        for uninit in buf.spare_capacity_mut().iter_mut() {
            unsafe {
                uninit.write(core::mem::zeroed());
            }
        }
        unsafe {
            buf.set_len(capacity);
        }
        Self {
            buf,
            _phantom: PhantomData::<Trb>,
        }
    }

    /// Return the head address of ring.
    fn as_ptr(&self) -> *const [u32; 4] {
        self.buf.as_ptr()
    }

    fn head_addr(&self) -> u64 {
        self.as_ptr() as u64
    }

    fn size(&self) -> usize {
        self.buf.len()
    }

    fn last_index(&self) -> usize {
        self.size() - 1
    }
}

impl<Trb: SoftwareProduceTrb> Ring<Trb> {
    fn set(&mut self, trb: Trb, index: usize) {
        self.buf[index] = trb.into_raw();
    }
}

impl Ring<TrbE> {
    /// Get TRB at the specified entry.
    ///
    /// # Panic
    /// If specified entry's memory representation is invalid.
    fn get(&self, index: usize) -> TrbE {
        self.buf[index].try_into().unwrap()
    }
}

#[derive(Debug)]
pub struct Producer<Trb: SoftwareProduceTrb> {
    ring: Ring<Trb>,
    cycle_bit: bool,
    write_index: usize,
}
impl<Trb: SoftwareProduceTrb> Producer<Trb> {
    pub fn new(capacity: usize) -> Self {
        assert!(capacity >= 2, "TRB producer must have capacity >= 2, due to its requirement of Link TRB in last entry.");
        Self {
            ring: Ring::new(capacity),
            cycle_bit: true,
            write_index: 0,
        }
    }

    pub fn push(&mut self, trb: Trb) {
        self.ring.set(trb, self.write_index);
        self.write_index += 1;
        if self.write_index == self.ring.last_index() {
            let link_trb = Trb::link_trb();
            self.ring.set(link_trb, self.write_index);
            self.write_index = 0;
        }
    }

    pub fn producer_cycle_state(&self) -> bool {
        self.cycle_bit
    }

    pub fn head_addr(&self) -> u64 {
        self.ring.head_addr()
    }
}

#[derive(Debug)]
pub struct Consumer<Trb: SoftwareConsumeTrb> {
    ring: Ring<Trb>,
    cycle_bit: bool,
}
impl Consumer<TrbE> {
    pub fn new(capacity: usize) -> Self {
        Self {
            ring: Ring::new(capacity),
            cycle_bit: true,
        }
    }

    fn consumer_cycle_state(&self) -> bool {
        self.cycle_bit
    }

    fn toggle_consumer_cycle_state(&mut self) {
        self.cycle_bit = !self.cycle_bit;
    }

    #[must_use = "Event iterator must be used."]
    pub fn consume(&mut self, start_addr: u64) -> EventRingIterator {
        EventRingIterator::new(self, start_addr)
    }

    pub fn head_addr(&self) -> u64 {
        self.ring.head_addr()
    }

    pub fn size(&self) -> usize {
        self.ring.size()
    }

    pub fn get(&self, index: usize) -> TrbE {
        self.ring.get(index)
    }

    fn addr_at(&self, index: usize) -> u64 {
        self.ring.head_addr() + index as u64 / 16
    }
}

#[derive(Debug)]
pub struct EventRingIterator<'ring> {
    consumer: &'ring mut Consumer<TrbE>,
    index: usize,
    consumed: bool,
}
impl<'ring> EventRingIterator<'ring> {
    fn new(consumer: &'ring mut Consumer<TrbE>, start_addr: u64) -> Self {
        // Because ring buffer always 64 byte aligned, this operation is
        // equivalent to masking last 4 bit of `start_addr` and directly
        // access via memory address.
        let index = usize::try_from((start_addr - consumer.ring.head_addr()) / 16).unwrap();
        Self {
            consumer,
            index,
            consumed: false,
        }
    }

    /// Consume iterator and get pointer to write back to Event Ring Deque Pointer(ERDP) Register
    /// # Panics
    /// If call this function before iterating over this [`EventRingIterator`]
    /// (i.e. unprocessed Event TRB left).
    #[must_use = "Returned address must be written back to ERDP Register."]
    pub fn dequeue_pointer(self) -> u64 {
        assert!(
            self.consumed,
            "Cannot return pointer to write back to ERDP before consume TRBs."
        );
        self.consumer.addr_at(self.index)
    }
}
impl Iterator for EventRingIterator<'_> {
    type Item = TrbE;
    fn next(&mut self) -> Option<Self::Item> {
        if self.consumed {
            return None;
        }
        let trb = self.consumer.get(self.index);
        if trb.cycle_bit() == self.consumer.consumer_cycle_state() {
            if self.index == self.consumer.ring.last_index() {
                self.consumer.toggle_consumer_cycle_state();
                self.index = 0;
            } else {
                self.index += 1;
            }
            Some(trb)
        } else {
            self.consumed = true;
            None
        }
    }
}

#[derive(Debug, Default)]
#[repr(C)]
pub struct EventRingSegmentTableEntry {
    base_addr: u64,
    size: u16,
    _resv1: u16,
    _resv2: u32,
}
impl EventRingSegmentTableEntry {
    const fn zeroed() -> Self {
        Self {
            base_addr: 0,
            size: 0,
            _resv1: 0,
            _resv2: 0,
        }
    }
}

#[derive(Debug)]
pub struct EventRingSegmentTable {
    // Note: inner.push() can incur reallocation and it will bring difficult bugs.
    inner: Vec<EventRingSegmentTableEntry, RingAlloc>,
}
impl EventRingSegmentTable {
    pub fn new(er_segments: &[&EventRing]) -> Self {
        let capacity = er_segments.len();
        assert!(
            capacity > 0,
            "`EventRingSegmentTable` must have positive capacity.",
        );
        assert!(
            capacity <= u16::MAX as usize,
            "Capacity of `EventRingSegmentTable` must be equal or smaller than {}",
            u16::MAX,
        );

        let mut inner = Vec::with_capacity_in(capacity, ALLOC);
        for er in er_segments.iter() {
            let base_addr = er.head_addr();
            let size = er.size() as u16;
            inner.push(EventRingSegmentTableEntry {
                base_addr,
                size,
                ..Default::default()
            });
        }
        Self { inner }
    }

    pub fn size(&self) -> u16 {
        self.inner.capacity() as u16
    }

    fn head_ptr(&self) -> *const EventRingSegmentTableEntry {
        self.inner.as_ptr()
    }

    pub fn head_addr(&self) -> u64 {
        self.head_ptr() as u64
    }
}
