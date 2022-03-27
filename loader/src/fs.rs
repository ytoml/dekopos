//! Wrapper for uefi's file system
use uefi::prelude::BootServices;
use uefi::proto::media::file::Directory;
use uefi::{Handle, Result};

pub fn open_root_dir(image: Handle, boot: &BootServices) -> Result<Directory> {
    let fs = boot
        .get_image_file_system(image)
        .expect("failed to get fs.");
    unsafe { &mut *fs.interface.get() }.open_volume()
}
