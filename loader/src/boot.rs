use core::mem;
use core::slice;

use goblin::elf::{program_header, Elf};
use log::info;
use uefi::data_types::Align;
use uefi::proto::media::file::{Directory, File, FileAttribute, FileInfo, FileMode, FileType};
use uefi::table::boot::{AllocateType, MemoryDescriptor, MemoryType};
use uefi::table::Runtime;
use uefi::{prelude::*, CString16, Result};

// Ugly hack?:
// According to elf header, it seems to be better to load elf at 0x101000 rather than at 0x100000,
// while link option specifies base address as 0x100000...
const KERNEL_BASE_ADDR: usize = 0x101000;
// const KERNEL_BASE_ADDR: usize = 0x100000;
const EFI_PAGE_SIZE: usize = 0x1000; // 4096 B
const ELF_ENTRY_OFFSET: usize = 0x18;

/// Loading kernel executable.
/// Return value is address of entry point.
pub(crate) fn load_kernel(root: &mut Directory, boot: &BootServices) -> Result<*const u8> {
    let filename = CString16::try_from("kernel.elf").unwrap();
    let mut file = match root
        .open(&filename, FileMode::Read, FileAttribute::empty())?
        .into_type()?
    {
        FileType::Regular(file) => file,
        FileType::Dir(_) => panic!("entry for \"kernel.elf\" is already exists as a directory."),
    };

    // Unlike C, (maybe) we cannot extract the size of ?Sized struct excluding last ?Sized member.
    // Also, we can assume that the file name of kernel binary doesn't differ.
    // Therefore, buffer length is hardcoded here, instead of getting size with intentional error.
    // let bufsize = file.get_info::<FileInfo>(&mut []).expect_err("");
    // let mut buf = vec![0; bufsize];
    let mut buf = [0; 102];
    let typebuf = <FileInfo as Align>::align_buf(&mut buf)
        .expect("cannot find good aligned buffer for filetype.");

    let size = file
        .get_info::<FileInfo>(typebuf)
        .expect("cannot get file info")
        .file_size() as usize;

    let mut src = vec![0; size];
    let _ = file.read(&mut src).expect("cannot read kernel executable.");
    load_elf(&src, boot)
}

pub(crate) fn load_elf(src: &[u8], boot: &BootServices) -> Result<*const u8> {
    let elf = Elf::parse(src).expect("failed to parse elf");
    let load_segments = elf
        .program_headers
        .iter()
        .filter(|ph| ph.p_type == program_header::PT_LOAD);

    let mut start_addr = usize::MAX;
    let mut end_addr = usize::MIN;
    for ph in load_segments {
        start_addr = start_addr.min(ph.p_vaddr as usize);
        end_addr = end_addr.max((ph.p_vaddr + ph.p_memsz) as usize);
    }
    let kern_size = (end_addr - start_addr) as usize;
    let _ = boot.allocate_pages(
        AllocateType::Address(start_addr),
        MemoryType::LOADER_DATA,
        (kern_size + EFI_PAGE_SIZE - 1) / EFI_PAGE_SIZE, // Round upping
    )?;

    let load_segments = elf
        .program_headers
        .iter()
        .filter(|ph| ph.p_type == program_header::PT_LOAD);
    for ph in load_segments {
        let of = ph.p_offset as usize;
        let msiz = ph.p_memsz as usize;
        let fsiz = ph.p_memsz as usize;
        let vaddr = ph.p_vaddr as *mut u8;

        let dst = unsafe { slice::from_raw_parts_mut(vaddr, msiz) };
        dst.copy_from_slice(&src[of..of + fsiz]);
        dst[fsiz..].fill(0);
    }

    info!(
        "Elf loaded: Load segment = {:#08x} - {:#08x}",
        start_addr, end_addr
    );
    Ok(elf.entry as *const u8)
}

pub(crate) fn exit_boot_services(
    image: Handle,
    systab: SystemTable<Boot>,
) -> Result<SystemTable<Runtime>> {
    let size =
        systab.boot_services().memory_map_size().map_size + 8 * mem::size_of::<MemoryDescriptor>();
    let mut mmap_buf = vec![0; size];
    let (runtime, _) = systab.exit_boot_services(image, &mut mmap_buf)?;
    // Note that allocator can't be used anymore after boot service exits,
    // and we have to tell Rust not to try to drop buffer that was allocated by uefi service.
    mem::forget(mmap_buf);
    Ok(runtime)
}

/// Functionalities which were implemented in past chapters.
mod unused {
    #![allow(unused)]
    use uefi::proto::media::file::RegularFile;
    use uefi::table::boot::MemoryDescriptor;

    const MEMMAP_SIZE: usize = 4096 * 4;
    const HEADER: &[u8; 65] = b"Index, Type, Type(name), PhysicalStart, NumberOfPages, Attribute\n";

    /// Dump memmap to file
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
            file.write(line.as_bytes())
                .expect("failed to write to file.");
        }
    }
}
