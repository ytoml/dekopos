#![feature(default_alloc_error_handler)]
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
pub extern "sysv64" fn kernel_main(
    mmap: *const ::common_data::mmap::MemMap,
    fb: *mut ::common_data::graphics::FrameBuffer,
) {
    unsafe { services::init(mmap, fb) };
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

    detect_usb();
    inspect_memmap();

    hlt!();
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
    let pci_devices = unsafe { &mut PCI_DEVICES };
    if let Err(e) = pci_devices.scan_all_bus() {
        kprintln!("[WARN]: {:?}", e);
    }

    kprintln!();
    kprintln!("Detected devices:");
    for (i, device) in pci_devices.iter().flatten().enumerate() {
        kprintln!(
            "[{}] {:02}.{:02}.{:02}: vendor={:#06x}, class={:#010x}, header={:#04x}",
            i,
            device.bus(),
            device.device_number(),
            device.function(),
            device.vendor_id().as_raw(),
            device.class_code().as_raw(),
            device.header_type().as_raw(),
        )
    }
    pci_devices.reset();
}

fn detect_usb() {
    use services::PCI_DEVICES;
    let pci_devices = unsafe { &mut PCI_DEVICES };
    if let Err(e) = pci_devices.scan_all_bus() {
        kprintln!("[WARN]: {:?}", e);
    }

    let mut usb = None;
    for device in pci_devices.iter().flatten() {
        if device.class_code().is_usb() {
            kprintln!("USB detected!: {:?}", device);
            kprintln!("MMIO: {:?}", device.bar(0));
            usb.insert(*device);
            if device.vendor_id().is_intel() {
                break;
            }
        }
    }

    if usb.is_none() {
        kprintln!("USB unavailable...");
    }

    pci_devices.reset();
}

fn inspect_memmap() {
    use services::MMAP;
    let mmap = unsafe { MMAP.as_ref().unwrap() };
    kprintln!("{:?}", mmap);
    kprintln!("index, type, phys_start...phys_end,   offset,  att");
    for (i, desc) in mmap.as_slice().iter().enumerate() {
        kprintln!(
            "{:02},    {:#03x}, {:#010x}..{:#010x}, {:#08x}, {:#08x}",
            i,
            desc.ty,
            desc.phys_start,
            desc.phys_end,
            desc.offset,
            desc.attribute
        );
    }
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    kprintln!("{}", info);
    hlt!();
}

#[macro_export]
macro_rules! hlt {
    () => {{
        #[allow(unused_unsafe)]
        unsafe {
            loop {
                asm!("hlt");
            }
        }
    }};
}
