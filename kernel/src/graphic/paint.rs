use super::font;
use super::FrameBuffer;

pub trait Paint {
    /// fill the continuous elements with RGB properties.
    /// structs which implement this trait are required to have RGB properties.
    /// Note that this is static method and structs implement this trait cannot use member variables.
    fn paint(pixel: &mut [u8], c: Color);
}

#[derive(Debug, Default, Clone, Copy)]
pub struct Color {
    r: u8,
    g: u8,
    b: u8,
}

impl Color {
    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct Bgr;

impl Paint for Bgr {
    fn paint(pixel: &mut [u8], c: Color) {
        pixel[0] = c.b;
        pixel[1] = c.g;
        pixel[2] = c.r;
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct Rgb;

impl Paint for Rgb {
    fn paint(pixel: &mut [u8], c: Color) {
        pixel[0] = c.r;
        pixel[1] = c.g;
        pixel[2] = c.b;
    }
}

type Painter = fn(&mut [u8], Color);
type PainterWithLifeTime<'a> = fn(&'a mut [u8], Color);

pub struct FrameBufPainter<'fb> {
    pub(super) fb: &'fb mut FrameBuffer,
    pub(super) painter: Painter,
}

impl<'fb> FrameBufPainter<'fb> {
    pub fn new(fb: &'fb mut FrameBuffer, painter: Painter) -> Self {
        Self { fb, painter }
    }

    pub fn paint(&mut self, x: usize, y: usize, color: Color) {
        let i = self.fb.index(x, y);
        (self.painter)(&mut self.fb.inner_slice_mut()[i..i + 3], color);
    }

    pub(super) fn paint_all(&mut self, color: Color) {
        let (w, h) = self.fb.resolution();
        for x in 0..w {
            for y in 0..h {
                self.paint(x, y, color);
            }
        }
    }

    pub fn paint_ascii(&mut self, c: char, x: usize, y: usize, color: Color) {
        let ascii = font::get_font(c);
        for (dy, &layout) in ascii.as_slice().iter().enumerate() {
            let mut l = layout;
            let mut dx = 0;
            while l != 0 {
                if l & 0x80 != 0 {
                    self.paint(x + dx, y + dy, color);
                }
                dx += 1;
                l <<= 1;
            }
        }
    }
}

// This implementation can write on only single window now(i.e. cannot scroll).
impl<'fb> core::fmt::Debug for FrameBufPainter<'fb> {
    fn fmt<'a>(&'a self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("FrameBufPainter")
            .field("fb", &self.fb)
            .field("painter", &self.painter as &PainterWithLifeTime<'a>)
            .finish()
    }
}
