extern crate alloc;

use core::ptr;

use xhci::context::{Device32Byte, Endpoint32Byte, Input32Byte, InputControl32Byte, Slot32Byte};

use super::usb::mem::BoundedAlloc64;

pub const CONTEXT_SIZE: usize = 32;
pub const CSZ: bool = CONTEXT_SIZE == 64;
const BOUNDARY: u64 = 4096;
type Alloc = BoundedAlloc64;
type Vec<T> = alloc::vec::Vec<T, Alloc>;
pub type DeviceCtx = Device32Byte;
pub type EndpointCtx = Endpoint32Byte;
pub type InputCtx = Input32Byte;
pub type InputCtrlCtx = InputControl32Byte;
pub type SlotCtx = Slot32Byte;

#[derive(Debug)]
pub struct DeviceContextBaseAddressArray {
    // NOTE: Box contains allocator, thus it's size is > 8 if Allocator is not zero-sized.
    // Thus, we do not use Box here
    inner: Vec<*mut DeviceCtx>,
}

impl DeviceContextBaseAddressArray {
    pub fn new(capacity: usize) -> Self {
        assert!(
            (1..=255).contains(&capacity),
            "DCBAA must be size in 1..=255, but {capacity} passed."
        );
        // Rust's null is defined as 0x0 and this fits xHCI specification
        let inner = vec_no_realloc![ptr::null_mut::<DeviceCtx>(); capacity; Alloc::new(BOUNDARY)];

        // NOTE: Box contains allocator, thus it's size is > 8 if Allocator is not zero-sized.
        Self { inner }
    }

    /// Caller must ensure that the registered [`DeviceCtx`] (pointee on the heap) is managed (not destroyed).
    pub fn register(&mut self, i: u8, device: *mut DeviceCtx) {
        // Ugly hack: to make allocation simpler, downcasting here.
        // This is due to demandings of filling array with 0 (see implementation of new() above).
        assert!(i > 0, "The very head(index=0) of DCBAA is not for use.");
        let i: usize = i.into();
        self.inner[i] = device;
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
