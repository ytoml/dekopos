use common_data::graphic::PixelFormat;

/// Assume that identical memory representation to [`common_data::graphic::FrameBuffer`]
#[repr(C)]
pub struct FrameBuffer(::common_data::graphic::FrameBuffer);

impl From<::common_data::graphic::FrameBuffer> for FrameBuffer {
    fn from(fb: ::common_data::graphic::FrameBuffer) -> Self {
        Self(fb)
    }
}

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

pub struct FrameBufPainter<'fb> {
    fb: &'fb mut FrameBuffer,
    painter: fn(&mut [u8], Color),
}

impl<'fb> FrameBufPainter<'fb> {
    pub fn paint(&mut self, x: usize, y: usize, c: Color) {
        let i = self.fb.index(x, y);
        (self.painter)(&mut self.fb.inner_slice_mut()[i..i + 3], c);
    }
}

impl FrameBuffer {
    #[inline]
    fn inner_slice_mut(&mut self) -> &mut [u8] {
        unsafe { self.0.as_mut_slice() }
    }

    #[inline]
    pub fn index(&self, x: usize, y: usize) -> usize {
        (self.stride() * y + x) * 4
    }

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
}

impl<'fb> FrameBuffer {
    pub fn painter(&'fb mut self) -> FrameBufPainter<'fb> {
        let painter = match self.format() {
            PixelFormat::Bgr => Bgr::paint,
            PixelFormat::Rgb => Rgb::paint,
        };
        FrameBufPainter { fb: self, painter }
    }
}
