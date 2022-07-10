#![allow(dead_code, unused)]
extern crate alloc;

use alloc::alloc::{GlobalAlloc, Layout};

struct OsAllocator;

unsafe impl GlobalAlloc for OsAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        todo!()
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        todo!()
    }
}

#[global_allocator]
static GLOBAL_ALLOC: OsAllocator = OsAllocator;
