#![no_std]
#![no_main]
#![feature(abi_efiapi)]

#[macro_use]
extern crate alloc;
extern crate uefi_services;

use log::info;
use uefi::table::runtime::ResetType;
use uefi::{prelude::*, CString16};
use uefi::proto::media::file::{File, FileAttribute, FileMode, FileType, RegularFile};
use uefi::table::boot::MemoryDescriptor;

const MEMMAP_SIZE: usize = 4096 * 4;

#[entry]
fn efi_main(img: Handle, mut systab: SystemTable<Boot>) -> Status {
    uefi_services::init(&mut systab).unwrap(); // initialization for alloc/logger

    info!("getting memory map...");
    let mut mapbuf = [0u8; MEMMAP_SIZE];
    let (_map, memdesc) = systab
        .boot_services()
        .memory_map(&mut mapbuf)
        .expect("failed to get memmap");

    info!("accessing file system...");
    let mut root = {
        let fs = systab
            .boot_services()
            .get_image_file_system(img)
            .expect("failed to get fs.");
        unsafe { &mut *fs.interface.get() }
            .open_volume()
            .expect("failed to open volume.")
    };

    info!("opening file...");
    let filename = CString16::try_from("memmap").unwrap();
    let file = match root
        .open(&filename, FileMode::CreateReadWrite, FileAttribute::empty())
        .expect("failed to open file \"memmap\".")
        .into_type()
        .unwrap()
    {
        FileType::Regular(file) => file,
        FileType::Dir(_) => panic!("entry for \"memmap\" is already exists as a directory."),
    };

    info!("saving memmap...");
    save_memmap(memdesc, file);
    info!("succeeded.");

    systab.boot_services().stall(3_000_000);
    systab.stdout().reset(false).unwrap();
    systab.runtime_services().reset(ResetType::Shutdown, Status::SUCCESS, None);
}

const HEADER: &[u8; 65] = b"Index, Type, Type(name), PhysicalStart, NumberOfPages, Attribute\n";

fn save_memmap<'a, M>(desc: M, mut file: RegularFile)
where
    M: ExactSizeIterator<Item = &'a MemoryDescriptor> + Clone,
{
    // It is OK to write u8 because user will read this file through other machine rather than this application runs on (e.g. Host for QEMU).
    file.write(HEADER).expect("failed to write to file.");
    for (i, d) in desc.enumerate() {
        let line = format!(
            "{}, {:#x}, {:?}, {:#08x}, {:#x}, {:#x}\n",
            i,
            d.ty.0,
            d.ty,
            d.phys_start,
            d.virt_start,
            d.att.bits().clamp(0, 0xfffff)
        );
        file.write(line.as_bytes()).expect("failed to write to file.");
    }
}
