use core::cell::UnsafeCell;

use uefi::prelude::*;
use uefi::proto::console::gop::GraphicsOutput;
use uefi::Result;

pub fn open_gop(boot: &BootServices) -> Result<&UnsafeCell<GraphicsOutput>> {
    boot.locate_protocol::<GraphicsOutput>()
}

pub fn paint_white_all(gop: &mut GraphicsOutput) {
    let mut fb = gop.frame_buffer();
    for i in 0..fb.size() {
        unsafe { fb.write_byte(i, 255); }
    }
}


