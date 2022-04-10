use common_data::graphics::PixelFormat;

use super::font;
use super::{Bgr, Color, Draw, Offset, Paint, Position, Rgb};

#[derive(Debug)]
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
    pub fn drawer(&'fb mut self) -> FrameBufDrawer<'fb> {
        FrameBufDrawer::new(self)
    }
}

impl From<::common_data::graphics::FrameBuffer> for FrameBuffer {
    fn from(fb: ::common_data::graphics::FrameBuffer) -> Self {
        Self(fb)
    }
}

type Painter = fn(&mut [u8], Color);
type PainterWithLifeTime<'a> = fn(&'a mut [u8], Color);

pub struct FrameBufDrawer<'fb> {
    pub(super) fb: &'fb mut FrameBuffer,
    pub(super) painter: Painter,
}

impl<'fb> FrameBufDrawer<'fb> {
    pub fn new(fb: &'fb mut FrameBuffer) -> Self {
        let painter = match fb.format() {
            PixelFormat::Bgr => Bgr::paint,
            PixelFormat::Rgb => Rgb::paint,
        };
        Self { fb, painter }
    }

    pub(super) fn draw_all(&mut self, color: Color) {
        let lower_right = self.fb.resolution().into();
        self.fill_rect(Position::zero(), lower_right, color);
    }

    pub fn draw_ascii(&mut self, c: char, start: Position, color: Color) {
        let ascii = font::get_font(c);
        for (dy, &layout) in ascii.as_slice().iter().enumerate() {
            let mut l = layout;
            let mut dx = 0;
            while l != 0 {
                if l & 0x80 != 0 {
                    let p = start + Offset::new(dx, dy);
                    self.draw_pixel(p, color);
                }
                dx += 1;
                l <<= 1;
            }
        }
    }
}

impl<'fb> Draw for FrameBufDrawer<'fb> {
    fn draw_pixel(&mut self, p: Position, color: Color) {
        let i = self.fb.index(p.x, p.y);
        (self.painter)(&mut self.fb.inner_slice_mut()[i..i + 3], color);
    }
}

// This implementation can write on only single window now(i.e. cannot scroll).
impl<'fb> core::fmt::Debug for FrameBufDrawer<'fb> {
    fn fmt<'a>(&'a self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("FrameBufDrawer")
            .field("fb", &self.fb)
            .field("painter", &self.painter as &PainterWithLifeTime<'a>)
            .finish()
    }
}
