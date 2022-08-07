extern crate alloc;

use core::alloc::{AllocError, Allocator, Layout};
use core::num::NonZeroUsize;
use core::ptr::NonNull;

use accessor::Mapper;
use spin::Mutex;

use crate::utils::{Aligned64, PageAligned};
use crate::x64;

pub(super) type ReadWriteArray<T> = accessor::array::ReadWrite<T, UsbMapper>;
pub(super) type ReadWrite<T> = accessor::single::ReadWrite<T, UsbMapper>;

// Note that this mapper is just for memory-mapped IO
// and is different from virtual address mapper for page table.
#[derive(Debug, Clone)]
pub struct UsbMapper;
impl Mapper for UsbMapper {
    unsafe fn map(&mut self, phys_start: usize, _bytes: usize) -> NonZeroUsize {
        NonZeroUsize::new(phys_start).expect("physical address 0 is passed to mapper.")
    }

    fn unmap(&mut self, _virt_start: usize, _bytes: usize) {}
}

pub(super) type Vec<T> = alloc::vec::Vec<T, UsbAllocator>;
pub(super) type Box<T> = alloc::boxed::Box<T, UsbAllocator>;
pub type AlignedBox64<T> = alloc::boxed::Box<T, UsbAlignedAllocator<64>>;
pub type BoundedBox64<T> = alloc::boxed::Box<T, UsbBoundedAllocator<64>>;
pub type BoundedAlloc64 = UsbBoundedAllocator<64>;

#[derive(Debug, Clone, Copy)]
pub struct UsbAllocator;
unsafe impl Allocator for UsbAllocator {
    fn allocate(&self, layout: Layout) -> Result<NonNull<[u8]>, AllocError> {
        unsafe { heap_mut() }.alloc(layout).ok_or(AllocError)
    }

    unsafe fn deallocate(&self, _ptr: NonNull<u8>, _layout: Layout) {}
}

/// Allocator with static alignment.
#[derive(Debug, Clone, Copy)]
pub struct UsbAlignedAllocator<const ALIGN: usize>;
unsafe impl<const ALIGN: usize> Allocator for UsbAlignedAllocator<ALIGN> {
    fn allocate(&self, layout: Layout) -> Result<NonNull<[u8]>, AllocError> {
        UsbAllocator.allocate(layout.align_to(ALIGN).unwrap())
    }

    unsafe fn deallocate(&self, _ptr: NonNull<u8>, _layout: Layout) {}
}

/// Allocator with dynamic page boundary (indicate to handle Page Size Register, etc.)
#[derive(Debug, Clone, Copy)]
pub struct UsbBoundedAllocator<const ALIGN: usize> {
    boundary: u64,
}
impl<const ALIGN: usize> UsbBoundedAllocator<ALIGN> {
    pub const fn new(boundary: u64) -> Self {
        Self { boundary }
    }
}

unsafe impl<const ALIGN: usize> Allocator for UsbBoundedAllocator<ALIGN> {
    fn allocate(&self, layout: Layout) -> Result<NonNull<[u8]>, AllocError> {
        unsafe { heap_mut() }
            .alloc_with_boundary(layout.align_to(ALIGN).unwrap(), self.boundary)
            .ok_or(AllocError)
    }

    unsafe fn deallocate(&self, _ptr: NonNull<u8>, _layout: Layout) {}
}

/// only one allocation
pub struct OneShotHeap<const LIMIT: usize> {
    pool: Aligned64<[u8; LIMIT]>,
    cur: usize,
    mu: Mutex<()>,
}

impl<const LIMIT: usize> OneShotHeap<LIMIT> {
    const fn new() -> Self {
        Self {
            pool: Aligned64::new([0; LIMIT]),
            cur: 0,
            mu: Mutex::new(()),
        }
    }

    fn alloc(&mut self, layout: Layout) -> Option<NonNull<[u8]>> {
        let _guard = self.mu.lock();
        let align = layout.align() as u64;
        let size = layout.size();
        log::debug!("cur = {}, size = {}, align = {}", self.cur, size, align);
        if self.cur >= LIMIT {
            return None;
        }
        let head = (self.cursor_ptr_aligned_up(align) - self.base_ptr_u64()) as usize;
        let newcur = head + size;
        // It's OK to be newcur == LIMIT (allocation is for ..=LIMIT-1 this time)
        if newcur > LIMIT {
            return None;
        }
        self.cur = newcur;
        NonNull::new(&mut self.pool[head..newcur] as *mut [u8])
    }

    fn alloc_with_boundary(&mut self, mut layout: Layout, boundary: u64) -> Option<NonNull<[u8]>> {
        let align = layout.align() as u64;
        let size = layout.size() as u64;
        log::debug!(
            "alloc_with_boundary: cur = {}, size = {}, align = {}, boundary: {}",
            self.cur,
            size,
            align,
            boundary
        );
        assert!(
            size <= boundary,
            "Allocating data size cannot be larger than boundary, but data size is {} and boundary is {}.",
            size,
            boundary
        );
        let ptr = self.cursor_ptr_aligned_up(align);
        let next_boundary = x64::align_up(ptr, boundary);
        if ptr + size >= next_boundary {
            layout = layout.align_to(boundary as usize).unwrap();
        }
        self.alloc(layout)
    }

    pub fn base_ptr_u64(&self) -> u64 {
        &self.pool[0] as *const u8 as u64
    }

    #[inline]
    fn cursor_ptr_u64(&self) -> u64 {
        &self.pool[self.cur] as *const u8 as u64
    }

    fn cursor_ptr_aligned_up(&self, align: u64) -> u64 {
        x64::align_up(self.cursor_ptr_u64(), align)
    }
}

const USB_MMAP_POOL_SIZE: usize = 4096 * 32;
type Heap = OneShotHeap<{ USB_MMAP_POOL_SIZE }>;
pub static mut HEAP: PageAligned<Heap> = PageAligned::new(Heap::new());
access_static_as_both!(heap, HEAP, Heap);
