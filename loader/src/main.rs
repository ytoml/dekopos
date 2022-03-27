#![no_std]
#![no_main]
#![feature(abi_efiapi)]

#[macro_use]
extern crate alloc;
extern crate uefi_services;

use log::info;
use uefi::prelude::*;

mod boot;
mod fs;
mod graphic;

const MEMMAP_SIZE: usize = 4096 * 4;

#[entry]
fn efi_main(image: Handle, mut systab: SystemTable<Boot>) -> Status {
    uefi_services::init(&mut systab).unwrap(); // initialization for alloc/logger
    reset!(systab, stdout, false);
    let boot = systab.boot_services();

    info!("getting memory map...");
    let mut mmap_buf = [0u8; MEMMAP_SIZE];
    assert!(mmap_buf.len() > boot.memory_map_size().map_size);

    let (_, _) = boot
        .memory_map(&mut mmap_buf)
        .expect("failed to get memmap");

    info!("getting graphic output protocol...");
    let gop = graphic::open_gop(boot).expect("failed to open graphic output protocol.");
    let gop = unsafe { &mut *gop.get() };
    let mode = gop.current_mode_info();
    info!(
        "Resolution: (w, h)={:?}, Pixel Format: {:?}, {} px/line",
        mode.resolution(),
        mode.pixel_format(),
        mode.stride()
    );
    graphic::paint_white_all(gop);

    info!("accessing file system...");
    let mut root = fs::open_root_dir(image, boot).expect("failed to open root directory");

    info!("loading kernel file...");
    let entry = boot::load_kernel(&mut root, boot).expect("failed to loading kernel.");

    info!("exit boot service...");
    let _ = boot::exit_boot_services(image, systab).expect("failed to exit boot service.");

    info!("calling kernel entry...");
    entry();

    #[allow(clippy::empty_loop)]
    loop {}
}

#[macro_export]
macro_rules! reset {
    ($system_table:ident, $stdio:ident, $extended:literal) => {{
        $system_table
            .$stdio()
            .reset($extended)
            .expect(concat!("failed to reset ", stringify!($ident)));
    }};
}
