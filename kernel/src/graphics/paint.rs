use super::Position;

pub trait Paint {
    /// fill the continuous elements with RGB properties.
    /// structs which implement this trait are required to have RGB properties.
    /// Note that this is static method and structs implement this trait cannot use member variables.
    fn paint(pixel: &mut [u8], c: Color);
}

pub trait Draw {
    fn draw_pixel(&mut self, p: Position, color: Color);

    fn draw_rect(&mut self, upper_left: Position, lower_right: Position, color: Color) {
        if upper_left.x == lower_right.x || upper_left.y == lower_right.y { return; }
        for x in upper_left.x..lower_right.x {
            self.draw_pixel(Position::new(x, upper_left.y), color);
            self.draw_pixel(Position::new(x, lower_right.y-1), color);
        }

        for y in upper_left.y+1..lower_right.y-1 {
            self.draw_pixel(Position::new(upper_left.x, y), color);
            self.draw_pixel(Position::new(lower_right.x-1, y), color)
        }
    }

    fn fill_rect(&mut self, upper_left: Position, lower_right: Position, color: Color) {
        for x in upper_left.x..lower_right.x {
            for y in upper_left.y..lower_right.y {
                self.draw_pixel(Position::new(x, y), color);
            }
        }
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct Color {
    r: u8,
    g: u8,
    b: u8,
}

#[allow(dead_code)]
impl Color {
    pub const BLACK: Self = Self::new(0, 0, 0);
    pub const RED: Self = Self::new(255, 0, 0);
    pub const GREEN: Self = Self::new(0, 255, 0);
    pub const BLUE: Self = Self::new(0, 0, 255);
    pub const WHITE: Self = Self::new(255, 255, 255);
}

impl Color {
    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub(super) struct Bgr;

impl Paint for Bgr {
    #[inline]
    fn paint(pixel: &mut [u8], c: Color) {
        pixel[0] = c.b;
        pixel[1] = c.g;
        pixel[2] = c.r;
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub(super) struct Rgb;

impl Paint for Rgb {
    #[inline]
    fn paint(pixel: &mut [u8], c: Color) {
        pixel[0] = c.r;
        pixel[1] = c.g;
        pixel[2] = c.b;
    }
}
