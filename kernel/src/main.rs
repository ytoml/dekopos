#![cfg_attr(not(test), no_std)]
#![no_main]

extern crate derive_more;

use core::arch::asm;
use core::panic::PanicInfo;

mod data_types;
#[macro_use]
mod graphics;
mod services;

use graphics::Color;

use crate::graphics::{Draw, Position};

#[no_mangle]
pub extern "sysv64" fn kernel_main(fb: *mut ::common_data::graphics::FrameBuffer) {
    services::init(fb);
    kprintln!("{}", HELLO_KERNEL);
    kprintln!(r"
______                     _____ _____ 
|  _  \                   |  _  /  ___|
| | | |___  ___ ___  _ __ | | | \ `--. 
| | | / _ \/ __/ _ \| '_ \| | | |`--. \
| |/ /  __/ (_| (_) | |_) \ \_/ /\__/ /
|___/ \___|\___\___/| .__/ \___/\____/ 
                    | |                
                    |_|                          
    ");
    draw_something();
    kprintln!("Screen successfully rendered!");

    loop {
        unsafe {
            asm!("hlt");
        }
    }
}

const HELLO_KERNEL: &str = "Hello, Kernel! This is OS kernel crafted with Rust. Have fun and I wish you learn much during implementing this. Good luck!";

fn draw_something() {
    use services::CONSOLE;
    let console = unsafe { CONSOLE.as_mut().unwrap() };
    console.drawer.fill_rect(Position::new(0, 500), Position::new(100, 600), Color::GREEN);
    console.drawer.fill_rect(Position::new(100,500), Position::new(800, 600), Color::BLACK);
    console.drawer.draw_rect(Position::new(10, 510), Position::new(90, 590), Color::WHITE);
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {
        unsafe {
            asm!("hlt");
        }
    }
}
