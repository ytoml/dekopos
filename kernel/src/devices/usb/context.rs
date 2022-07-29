extern crate alloc;
use core::pin::Pin;
use core::ptr;

use xhci::context::{Device32Byte, Endpoint32Byte, Input32Byte, InputControl32Byte, Slot32Byte};

use super::mem::XhcRuntimeAllocator;

pub const CONTEXT_SIZE: usize = 32;
pub const CSZ: bool = CONTEXT_SIZE == 64;
type Alloc = XhcRuntimeAllocator<64>;
type Box<T> = alloc::boxed::Box<T, Alloc>;
type Vec<T> = alloc::vec::Vec<T, Alloc>;
pub type DeviceCtx = Device32Byte;
pub type EndpointCtx = Endpoint32Byte;
pub type InputCtx = Input32Byte;
pub type InputCtrlCtx = InputControl32Byte;
pub type SlotCtx = Slot32Byte;

pub struct DeviceContextBaseAddressArray {
    // Note: inner.push() can incur reallocation and it will bring difficult bugs.
    inner: Vec<*mut DeviceCtx>,
}

impl DeviceContextBaseAddressArray {
    pub fn new(capacity: usize, boundary: u64) -> Self {
        // Rust's null is defined as 0x0 and this fits xHCI specification
        let inner = vec_no_realloc![ptr::null_mut::<DeviceCtx>(); capacity; Alloc::new(boundary)];

        // NOTE: Box contains allocator, thus it's size is > 8 if Allocator is not zero-sized.
        Self { inner }
    }

    /// Register context that allocated with 64 bytes alignment in the heap.
    pub fn register(&mut self, i: usize, device: Pin<Box<DeviceCtx>>) {
        // Ugly hack: to make allocation simpler, downcasting here.
        // This is due to demandings of filling array with 0 (see implementation of new() above).
        assert!(i > 0, "The very head(index=0) of DCBAA is not for use.");
        self.inner[i] = Box::into_raw(Pin::into_inner(device));
    }

    fn as_ptr(&self) -> *const *mut DeviceCtx {
        self.inner.as_ptr()
    }

    pub fn head_addr(&self) -> u64 {
        self.as_ptr() as u64
    }
}

static mut DCBAA: Option<DeviceContextBaseAddressArray> = None;
access_static_as_both_unwrap!(dcbaa, DCBAA, DeviceContextBaseAddressArray);
access_static_mut!(
    dcbaa_option_mut,
    DCBAA,
    Option<DeviceContextBaseAddressArray>
);

/// Initialize only once. Multiple calls will be ignored.
pub fn init_dcbaa(dcbaa: DeviceContextBaseAddressArray) {
    unsafe { dcbaa_option_mut() }.get_or_insert(dcbaa);
}
