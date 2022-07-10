use super::font::{FONT_H as FH, FONT_W as FW};
use super::Position;
use super::{Color, FrameBufDrawer, FrameBuffer};

const COLS: usize = 95;
const ROWS: usize = 30;
const X_PAD: usize = 10;
const Y_PAD: usize = 10;

#[derive(Debug)]
pub struct Console<'fb> {
    pub(crate) drawer: FrameBufDrawer<'fb>,
    // In Rust, char uses 32 bits each and its memory consuming than char array in C.
    // This implementation would be replaced in future, but now leave as this for simplicity.
    buf: [[char; COLS]; ROWS],
    x: usize,
    y: usize,
    background_color: Color,
    output_color: Color,
}

impl<'fb> Console<'fb> {
    pub const fn from_drawer(drawer: FrameBufDrawer<'fb>) -> Self {
        Self {
            drawer,
            buf: [['\0'; COLS]; ROWS],
            x: 0,
            y: 0,
            background_color: Color::WHITE,
            output_color: Color::BLACK,
        }
    }

    pub fn from_frame_buffer(fb: &'fb mut FrameBuffer) -> Self {
        Self::from_drawer(fb.drawer())
    }

    pub fn fill_screen(&mut self) {
        self.drawer.draw_all(self.background_color);
    }

    pub fn set_background_color(&mut self, color: Color) {
        self.background_color = color;
    }

    pub fn set_output_color(&mut self, color: Color) {
        self.output_color = color;
    }
}

impl<'fb> core::fmt::Write for Console<'fb> {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        let mut x = self.x;
        let mut y = self.y;
        for c in s.chars() {
            // Scroll and repaint before putting char,
            // if cursor reaches the bottom of the console.
            if y == ROWS {
                self.drawer.draw_all(self.background_color); // TODO: fill only area for console.
                for r in 0..ROWS - 1 {
                    self.buf[r] = self.buf[r + 1];
                    for (c, &ch) in self.buf[r].iter().enumerate() {
                        if ch == '\n' {
                            break;
                        }
                        let pos = font_aligned_position(c, r);
                        self.drawer.draw_ascii(ch, pos, self.output_color);
                    }
                }
                self.buf[ROWS - 1].fill('\0');
                y -= 1;
            }

            self.buf[y][x] = c; // Note that 'y' selects row and 'x' selects column.
            if c == '\n' {
                x = 0;
                y += 1;
            } else {
                let pos = font_aligned_position(x, y);
                self.drawer.draw_ascii(c, pos, self.output_color);
                if x == COLS - 1 {
                    x = 0;
                    y += 1;
                } else {
                    x += 1;
                }
            }
        }
        self.x = x;
        self.y = y;
        Ok(())
    }
}

// grid counts to potision on frame buffer
#[inline]
fn font_aligned_position(x: usize, y: usize) -> Position {
    (X_PAD + x * FW, Y_PAD + y * FH).into()
}

#[macro_export]
macro_rules! kprint {
    ($($arg:tt)*) => {{
        #[allow(unused_imports)]
        use core::fmt::Write as _;
        #[allow(unused_unsafe)]
        let console = unsafe { $crate::services::console_mut() };
        write!(console, $($arg)*).expect("printk failed.");
    }};
}

#[macro_export]
macro_rules! kprintln {
    () => {{
        $crate::kprint!("\n");
    }};

    ($fmt:expr) => {{
        $crate::kprint!(concat!($fmt, "\n"));
    }};

    ($fmt:expr, $($arg:tt)*) => {{
        $crate::kprint!(concat!($fmt, "\n"), $($arg)*);
    }};
}
