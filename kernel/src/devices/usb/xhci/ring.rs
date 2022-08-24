extern crate alloc;
use core::marker::PhantomData;
use core::mem;
use core::pin::Pin;

pub use xhci::ring::trb::command::Allowed as TrbC;
pub use xhci::ring::trb::event::Allowed as TrbE;
pub use xhci::ring::trb::transfer::Allowed as TrbT;
use xhci::ring::trb::Link;

use super::usb::mem::UsbAlignedAllocator;
use super::usb::status::{HcResetted, HcRunning};
use super::usb::{InterruptRegisters, Operational};

pub type TransferRing = Producer<TrbT>;
pub type CommandRing = Producer<TrbC>;

type TrbRaw = [u32; 4];
type RingAlloc = UsbAlignedAllocator<64>;
type Vec<T> = alloc::vec::Vec<T, RingAlloc>;
type Box<T> = alloc::boxed::Box<T, RingAlloc>;
const ALLOC: RingAlloc = UsbAlignedAllocator::<64>;

pub trait SoftwareProduceTrb: Sized {
    fn link_trb() -> Self;
    fn into_raw(self) -> TrbRaw;
    fn set_cycle_bit(&mut self, pcs: bool);
}
pub trait SoftwareConsumeTrb {}

fn link_trb_with_toggle() -> Link {
    let mut link = Link::new();
    link.set_toggle_cycle();
    link
}

/// Read trb from specified address
/// # Safety
/// `pointer` must be valid pointer.
// XXX: TOTALLY invalid cast
// 1. enum and its content is not same in terms of its memory layout (they have different size)
// 2. trb_pointer() functions return "physical" address (currently, it's identity mapping thus ok).
pub unsafe fn read_trb<Trb>(pointer: u64) -> core::result::Result<Trb, TrbRaw>
where
    Trb: TryFrom<TrbRaw, Error = TrbRaw>,
{
    (pointer as *const TrbRaw).read().try_into()
}

impl SoftwareProduceTrb for TrbC {
    fn link_trb() -> Self {
        Self::Link(link_trb_with_toggle())
    }
    fn into_raw(self) -> TrbRaw {
        self.into_raw()
    }
    fn set_cycle_bit(&mut self, pcs: bool) {
        if pcs {
            self.set_cycle_bit();
        } else {
            self.clear_cycle_bit();
        }
    }
}
impl SoftwareConsumeTrb for TrbE {}
impl SoftwareProduceTrb for TrbT {
    fn link_trb() -> Self {
        Self::Link(link_trb_with_toggle())
    }
    fn into_raw(self) -> TrbRaw {
        self.into_raw()
    }
    fn set_cycle_bit(&mut self, pcs: bool) {
        if pcs {
            self.set_cycle_bit();
        } else {
            self.clear_cycle_bit();
        }
    }
}

/// Wrapper of fixed size [`Vec`] for ring management implementation.
/// Main purpose is to prevent `push` on [`Vec`] which might incur difficult bug
/// through reallocation (i.e. location changes without notifying ring related registers).
/// (Event ring can be dynamically sized, but it will be managed through [`EventRingSegmentTable`] and [`Ring`] will be inctanciated per segment.)
/// [`TrbRaw`] is used instead of [`Allowed`] from [`xhci`] crate, because [`Allowed`] is enum and
/// it has metadata to distinguish variant, thus different size as TRB.
#[derive(Debug)]
struct Ring<Trb: TryFrom<TrbRaw>> {
    buf: Pin<Box<[TrbRaw]>>,
    _phantom: PhantomData<Trb>,
}
impl<Trb: TryFrom<TrbRaw>> Ring<Trb> {
    fn new(capacity: usize) -> Self {
        Self {
            buf: Pin::new(vec_no_realloc![[0; 4]; capacity; ALLOC].into_boxed_slice()),
            _phantom: PhantomData::<Trb>,
        }
    }

    /// Return the head address of ring.
    fn as_ptr(&self) -> *const TrbRaw {
        self.buf.as_ptr()
    }

    fn head_addr(&self) -> u64 {
        self.as_ptr() as u64
    }

    fn addr_at(&self, index: usize) -> u64 {
        self.head_addr() + (index * self.elem_size()) as u64
    }

    fn len(&self) -> usize {
        self.buf.len()
    }

    fn last_index(&self) -> usize {
        self.len() - 1
    }

    const fn elem_size(&self) -> usize {
        mem::size_of::<Trb>()
    }
}

impl<Trb: TryFrom<TrbRaw> + SoftwareProduceTrb> Ring<Trb> {
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
pub struct Producer<Trb: TryFrom<TrbRaw> + SoftwareProduceTrb> {
    ring: Ring<Trb>,
    cycle_bit: bool,
    write_index: usize,
}
impl Producer<TrbC> {
    /// # Safety
    /// User must ensure [`run_stop`] of USBCMD register is 0 when constructing this struct.
    /// If not, the xHCI's behavior will be undefined.
    /// See 5.4.5 of xHCI specification.
    pub unsafe fn new(capacity: usize, op: &mut Operational, _usb_status: &HcResetted) -> Self {
        assert!(capacity >= 2, "TRB producer must have capacity >= 2, due to its requirement of Link TRB in last entry.");

        let cr = Self {
            ring: Ring::new(capacity),
            cycle_bit: true,
            write_index: 0,
        };
        op.crcr.update_volatile(|r| {
            r.set_command_ring_pointer(cr.head_addr());
            // set same cycle bit (true) as command ring
            r.set_ring_cycle_state();
        });
        cr
    }
}

impl Producer<TrbT> {
    pub fn new(capacity: usize) -> Self {
        assert!(capacity >= 2, "TRB producer must have capacity >= 2, due to its requirement of Link TRB in last entry.");
        Self {
            ring: Ring::new(capacity),
            cycle_bit: true,
            write_index: 0,
        }
    }
}

impl<Trb: TryFrom<TrbRaw> + SoftwareProduceTrb> Producer<Trb> {
    /// This push returns address of pushed TRB for managing relationships
    /// between event and its issuer.
    /// Note that this function always sets producer cycle state to cycle bit of TRB,
    /// thus caller doesn't have to take care of it.
    pub fn push(&mut self, mut trb: Trb) -> u64 {
        trb.set_cycle_bit(self.producer_cycle_state());

        self.ring.set(trb, self.write_index);
        let trb_addr = self.ring.addr_at(self.write_index);
        self.write_index += 1;
        if self.write_index == self.ring.last_index() {
            let link_trb = Trb::link_trb();
            self.ring.set(link_trb, self.write_index);
            self.write_index = 0;
        }
        trb_addr
    }

    pub fn producer_cycle_state(&self) -> bool {
        self.cycle_bit
    }

    pub fn head_addr(&self) -> u64 {
        self.ring.head_addr()
    }
}

#[derive(Debug)]
pub struct EventRing {
    ring: Ring<TrbE>,
    seg_table: EventRingSegmentTable,
    cycle_bit: bool,
}

impl EventRing {
    /// Allocate event ring of single segment.
    ///
    /// # Safety
    /// User must ensure [`run_stop`] of USBCMD register is 0 when constructing this struct.
    /// If not, the xHCI's behavior will be undefined.
    /// See 5.5.2 of xHCI specification.
    pub unsafe fn new_primary(
        capacity: usize,
        intr: &mut InterruptRegisters,
        _usb_status: &HcResetted,
    ) -> Self {
        let ring = Ring::new(capacity);
        let seg_table = EventRingSegmentTable::new(&[&ring]);

        intr.update_volatile_at(0, |r| {
            r.erstsz.set(seg_table.size());
            r.erdp.set_event_ring_dequeue_pointer(ring.head_addr());
            r.erstba.set(seg_table.head_addr());
        });
        Self {
            ring,
            seg_table,
            cycle_bit: true,
        }
    }

    /// User must ensure event ring for primary interrupter is already created and registerd,
    /// and [`run_stop`] of USBCMD register is 1 when constructing this struct.
    pub unsafe fn new_secondary(
        _capacity: usize,
        _op: &mut Operational,
        _intr: &mut InterruptRegisters,
        _usb_status: &HcRunning,
    ) -> Self {
        todo!()
    }

    fn consumer_cycle_state(&self) -> bool {
        self.cycle_bit
    }

    fn toggle_consumer_cycle_state(&mut self) {
        self.cycle_bit = !self.cycle_bit;
    }

    pub fn is_unprocessed(&self, pointer: u64) -> bool {
        let index = self.index_of(pointer);
        self.get(index).cycle_bit() == self.consumer_cycle_state()
    }

    #[must_use = "Event iterator must be used."]
    pub fn consume(&mut self, start_addr: u64) -> EventRingIterator {
        EventRingIterator::new(self, start_addr)
    }

    pub fn head_addr(&self) -> u64 {
        self.ring.head_addr()
    }

    pub fn index_of(&self, addr: u64) -> usize {
        let head_addr = self.head_addr();
        assert!(addr > head_addr, "EventRing::index_of: addr must be >= head_addr, but {addr:#x} was passed while head_addr is {head_addr:#x}");
        assert!(
            addr % self.ring.elem_size() as u64 == 0,
            "EventRing::index_of: bad aligned addr is passed ({addr:#x})"
        );
        let index = (head_addr - addr) as usize / self.ring.elem_size();
        assert!(
            index < self.len(),
            "EventRing::index_of: index out of range"
        );
        index
    }

    pub fn len(&self) -> usize {
        self.ring.len()
    }

    pub fn get(&self, index: usize) -> TrbE {
        self.ring.get(index)
    }

    pub fn seg_table_head_addr(&self) -> u64 {
        self.seg_table.head_addr()
    }

    fn addr_at(&self, index: usize) -> u64 {
        self.ring.addr_at(index)
    }
}

#[derive(Debug)]
pub struct EventRingIterator<'ring> {
    consumer: &'ring mut EventRing,
    index: usize,
    consumed: bool,
}
impl<'ring> EventRingIterator<'ring> {
    fn new(consumer: &'ring mut EventRing, start_addr: u64) -> Self {
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
struct EventRingSegmentTable {
    // Note: inner.push() can incur reallocation and it will bring difficult bugs.
    inner: Vec<EventRingSegmentTableEntry>,
}
impl EventRingSegmentTable {
    fn new(er_segments: &[&Ring<TrbE>]) -> Self {
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
            inner.push(EventRingSegmentTableEntry {
                base_addr: er.head_addr(),
                size: er.len().try_into().unwrap(),
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
