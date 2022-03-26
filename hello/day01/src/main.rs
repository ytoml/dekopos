#![no_std]
#![no_main]
#![allow(unreachable_code)]

use core::{ffi::c_void, panic::PanicInfo};
use literal_expander::lazy_ucs2z;

type Char16 = u16;

// Note: when compile with "--target=x86_64-unknown-uefi", entry point function must be named as "efi_main" or lld will fail.
#[no_mangle]
extern "C" fn efi_main(_image_handle: EfiHandle, system_table: *mut EfiSystemTable) -> EfiStatus {
    unsafe {
        (*(*system_table).console_out).output.0(
            (*system_table).console_out,
            lazy_ucs2z!("Hello, world!\n").as_ptr(),
        );
    }
    loop {}
    EfiStatus(0)
}

#[repr(C)]
struct EfiHandle(*mut c_void);

#[repr(C)]
struct VoidPtr(*const c_void);

#[repr(C)]
struct EfiStatus(usize);

#[repr(C)]
struct EfiTextString(
    extern "C" fn(this: *const EfiSimpleTextOutputProtocol, *const Char16) -> EfiStatus,
);

#[repr(C)]
pub struct EfiSimpleTextOutputProtocol {
    dummy: VoidPtr,
    output: EfiTextString,
}

#[repr(C)]
pub struct Padding<const N: usize> {
    _inner: [u8; N],
}

#[repr(C)]
pub struct EfiSystemTable {
    // This program just ignores values below with Padding struct.
    // Header - 24 bytes
    // Pointer to firmware vendor - 8 bytes
    // Revision of UEFI specification - 4 bytes
    // Stdin handle (pointer) - 8 bytes
    // Pointer to input service - 8 bytes
    // Total = 52 bytes
    pad: Padding<52>,
    console_out_handle: EfiHandle,
    console_out: *mut EfiSimpleTextOutputProtocol,
    // Also, there should be
    //  - handle/service for stderr
    //  - pointer to boot service table
    //  - # of entries on configuration table
    //  - pointer to configuration table
    // This program just ignores them.
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {}
}
