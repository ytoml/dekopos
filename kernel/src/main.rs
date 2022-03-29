#![no_std]
#![no_main]

use core::panic::PanicInfo;
use core::arch::asm;

use common_data::graphic::FrameBuffer;

#[no_mangle]
pub extern "sysv64" fn kernel_main(fb: &mut FrameBuffer) {
    let screen = unsafe {
        fb.as_mut_slice()
    };

    for (i, pix) in screen.iter_mut().enumerate() {
            *pix = (i % 256) as u8;
    }

    loop {
        unsafe {
            asm!("hlt");
        }
    }
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {
        unsafe {
            asm!("hlt");
        }
    }
}
