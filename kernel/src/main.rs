#![no_std]
#![no_main]

use core::arch::asm;
use core::panic::PanicInfo;

#[macro_use]
mod graphics;
mod services;

#[no_mangle]
pub extern "sysv64" fn kernel_main(fb: *mut ::common_data::graphics::FrameBuffer) {
    services::init(fb);
    paint_all_chars();
    hello_kernel();

    loop {
        unsafe {
            asm!("hlt");
        }
    }
}

const VERY_LONG_STATEMENT: &str = "Hello, Kernel! This is OS kernel crafted with Rust. Have fun and I wish you learn much during implementing this. Good luck!";

fn paint_all_chars() {
    for c in (b'!'..=b'~').map(char::from) {
        kprint!("{}", c);
    }
    kprintln!();
}

fn hello_kernel() {
    for i in 0..100 {
        // confirm that console scrolling works...
        kprintln!("Loop {}", i);
    }
    kprintln!("{}", VERY_LONG_STATEMENT);
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {
        unsafe {
            asm!("hlt");
        }
    }
}
