#![feature(default_alloc_error_handler)]
#![no_std]
#![no_main]
#![feature(abi_x86_interrupt)]
#![feature(allocator_api)]
#![allow(unused_macros)]
#![allow(unreachable_code)] // for debug

extern crate derive_more;

use core::panic::PanicInfo;

#[macro_use]
mod utils;

mod data_types;
mod devices;
#[macro_use]
mod graphics;
mod mem;
mod services;
mod x64;

use crate::devices::pci::PciDevice;
use crate::graphics::{Color, Draw, Position};

pub use crate::utils::PageAligned;

#[no_mangle]
pub extern "sysv64" fn kernel_main(
    mmap: *const ::common_data::mmap::MemMap,
    fb: *mut ::common_data::graphics::FrameBuffer,
) -> ! {
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
    // draw_something();
    // kprintln!("Screen successfully rendered!");

    // scan_devices();
    // kprintln!("Devices successfully scanned!");

    let (mmio_base, device) = detect_usb();
    start_xhc(mmio_base as usize, device);
    // inspect_memmap();
    // debug();
    hlt!();
}

const HELLO_KERNEL: &str = "Hello, Kernel! This is OS kernel crafted with Rust. Have fun and I wish you learn much during implementing this. Good luck!";

fn scan_devices() {
    let pci_devices = unsafe { services::pci_devices_service_mut() };
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

fn detect_usb() -> (u64, PciDevice) {
    let pci_devices = unsafe { services::pci_devices_service_mut() };
    if let Err(e) = pci_devices.scan_all_bus() {
        kprintln!("[WARN]: {:?}", e);
    }

    let mut usb = None;
    let mut mmio_base = None;
    for device in pci_devices.iter().flatten() {
        if device.class_code().is_ehci() {
            kprintln!("EHCI: {:?}", device);
        }

        if device.class_code().is_usb_xhci() {
            kprintln!("USB detected!: {:?}", device);
            use devices::pci::Bar;
            match device.bar(0) {
                Bar::Memory64 { addr, .. } => {
                    kprintln!("MMIO: {:#018x}", addr);
                    let _ = mmio_base.insert(addr);
                }
                _ => {}
            }

            let _ = usb.insert(*device);
            if device.vendor_id().is_intel() {
                break;
            }
        }
    }

    (
        mmio_base.expect("USB unavailable."),
        usb.expect("USB unavailable."),
    )
}

fn inspect_memmap() {
    let mmap = unsafe { services::mmap() };
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

fn start_xhc(mmio_base: usize, device: PciDevice) {
    use devices::usb::HostController;
    let mut ctr = unsafe { HostController::new(mmio_base) };
    let pci_config = device.config();
    ctr.init(pci_config);
    ctr.start();

    // just for debug
    // use devices::interrupts;
    use devices::pci::msi::Capability;
    match pci_config.msi_capabilities().capability() {
        Capability::MsiX(c) => {
            log::info!("{c:#?}");
            let table = unsafe { c.table() };

            log::info!("{:#?}", table.read_volatile_at(0));
            let pba = unsafe { c.pending_bit_array() };
            log::info!("{pba:?}");
        }
        _ => {}
    }
}

fn debug() {
    let addr = crate::devices::interrupts::get_apic_addr(0);
    let x = unsafe { core::slice::from_raw_parts(addr as *const u8, 0x200) };
    log::info!("{x:?}");
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    kprintln!("{}", info);
    hlt!();
}

#[macro_export]
macro_rules! hlt {
    () => {{
        loop {
            x64::hlt();
        }
    }};
}
