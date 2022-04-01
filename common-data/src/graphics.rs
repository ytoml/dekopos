use core::slice;

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum PixelFormat {
    Bgr,
    Rgb,
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub struct FrameBuffer {
    base: *mut u8,
    pub size: usize,
    pub stride: usize,
    pub resolution: (usize, usize),
    pub format: PixelFormat,
}

impl FrameBuffer {
    /// # Safety
    /// This function can be used when caller believes FrameBuffer points valid address and have valid size.
    pub unsafe fn as_mut_slice(&mut self) -> &mut [u8] {
        slice::from_raw_parts_mut(self.base, self.size)
    }
}

/// Note that FrameBuffer can live longer than GraphicsOutput or its boot services
/// This leakage is intentional, but you should take care about it.
#[cfg(feature = "uefi_imp")]
impl<'gop> From<&mut uefi::proto::console::gop::GraphicsOutput<'gop>> for FrameBuffer {
    fn from(gop: &mut uefi::proto::console::gop::GraphicsOutput<'gop>) -> Self {
        let mut fb = gop.frame_buffer();
        let base = fb.as_mut_ptr();
        let size = fb.size();
        let mode = gop.current_mode_info();
        Self {
            base,
            size,
            stride: mode.stride(),
            resolution: mode.resolution(),
            format: mode.pixel_format().into(),
        }
    }
}

#[cfg(feature = "uefi_imp")]
impl From<uefi::proto::console::gop::PixelFormat> for PixelFormat {
    fn from(fmt: uefi::proto::console::gop::PixelFormat) -> Self {
        use uefi::proto::console::gop::PixelFormat as UefiPixelFormat;
        match fmt {
            UefiPixelFormat::Bgr => PixelFormat::Bgr,
            UefiPixelFormat::Rgb => PixelFormat::Rgb,
            _ => unimplemented!(),
        }
    }
}
