#![cfg_attr(not(test), no_std)]
#![no_main]

extern crate derive_more;

use core::arch::asm;
use core::panic::PanicInfo;

mod data_types;
mod devices;
#[macro_use]
mod graphics;
mod services;

use graphics::Color;

use crate::graphics::{Draw, Position};

#[no_mangle]
pub extern "sysv64" fn kernel_main(fb: *mut ::common_data::graphics::FrameBuffer) {
    services::init(fb);
    kprintln!("{}", HELLO_KERNEL);
    kprintln!(
        r"
______                     _____ _____ 
|  _  \                   |  _  /  ___|
| | | |___  ___ ___  _ __ | | | \ `--. 
| | | / _ \/ __/ _ \| '_ \| | | |`--. \
| |/ /  __/ (_| (_) | |_) \ \_/ /\__/ /
|___/ \___|\___\___/| .__/ \___/\____/ 
                    | |                
                    |_|                          
    "
    );
    draw_something();
    kprintln!("Screen successfully rendered!");

    scan_devices();
    kprintln!("Devices successfully scanned!");

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
    console
        .drawer
        .fill_rect(Position::new(0, 500), Position::new(100, 600), Color::GREEN);
    console.drawer.fill_rect(
        Position::new(100, 500),
        Position::new(800, 600),
        Color::BLACK,
    );
    console
        .drawer
        .draw_rect(Position::new(10, 510), Position::new(90, 590), Color::WHITE);
}

fn scan_devices() {
    use services::PCI_DEVICES;
    let pci_device = unsafe { PCI_DEVICES.as_mut().unwrap() };
    if let Err(e) = pci_device.scan_all_bus() {
        kprintln!("[WARN]: {:?}", e);
    }

    kprintln!();
    kprintln!("Detected devices:");
    for (i, device) in pci_device.iter().flatten().enumerate() {
        kprintln!(
            "[{}] {:02}.{:02}.{:02}: vendor={:#06x}, class={:#10x}, header={:#04x}",
            i,
            device.bus(),
            device.device_number(),
            device.function(),
            device.vendor_id().0,
            device.class_code().0,
            device.header_type().0,
        )
    }
}

static mut FIRST_PANIC: bool = true;
#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    unsafe {
        if FIRST_PANIC {
            // try to report panic information
            // If recursive panic occurs (i.e. panic due to CONSOLE), it will be quiet.
            FIRST_PANIC = false;
            if let Some(info) = info.payload().downcast_ref::<&str>() {
                kprintln!("{}", info);
            } else {
                kprintln!("panic occurred");
            }
        }

        loop {
            asm!("hlt");
        }
    }
}
