use common_data::graphics::PixelFormat;

#[macro_use]
pub mod console;
pub mod font;
mod paint;

pub use paint::*;

/// Assume that identical memory representation to [`common_data::graphic::FrameBuffer`]
#[derive(Debug)]
#[repr(C)]
pub struct FrameBuffer(::common_data::graphics::FrameBuffer);

impl FrameBuffer {
    #[inline]
    #[allow(dead_code)]
    pub const fn size(&self) -> usize {
        self.0.size
    }

    #[inline]
    pub const fn stride(&self) -> usize {
        self.0.stride
    }

    #[inline]
    pub const fn resolution(&self) -> (usize, usize) {
        self.0.resolution
    }

    #[inline]
    pub const fn format(&self) -> PixelFormat {
        self.0.format
    }

    #[inline]
    fn inner_slice_mut(&mut self) -> &mut [u8] {
        unsafe { self.0.as_mut_slice() }
    }

    #[inline]
    fn index(&self, x: usize, y: usize) -> usize {
        (self.stride() * y + x) * 4
    }
}

impl<'fb> FrameBuffer {
    pub fn painter(&'fb mut self) -> FrameBufPainter<'fb> {
        let painter = match self.format() {
            PixelFormat::Bgr => Bgr::paint,
            PixelFormat::Rgb => Rgb::paint,
        };
        FrameBufPainter::new(self, painter)
    }
}

impl From<::common_data::graphics::FrameBuffer> for FrameBuffer {
    fn from(fb: ::common_data::graphics::FrameBuffer) -> Self {
        Self(fb)
    }
}
