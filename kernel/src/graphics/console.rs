use super::font::{FONT_H as FH, FONT_W as FW};
use super::{Color, FrameBufPainter, FrameBuffer};

const COLS: usize = 95;
const ROWS: usize = 30;
const X_PAD: usize = 10;
const Y_PAD: usize = 10;

#[derive(Debug)]
pub struct Console<'fb> {
    painter: FrameBufPainter<'fb>,
    // In Rust, char uses 32 bits each and its memory consuming than char array in C.
    // This implementation would be replaced in future, but now leave as this for simplicity.
    buf: [[char; COLS]; ROWS],
    x: usize,
    y: usize,
}

impl<'fb> Console<'fb> {
    pub const fn from_painter(painter: FrameBufPainter<'fb>) -> Self {
        Self {
            painter,
            buf: [['\0'; COLS]; ROWS],
            x: 0,
            y: 0,
        }
    }

    pub fn from_frame_buffer(fb: &'fb mut FrameBuffer) -> Self {
        Self::from_painter(fb.painter())
    }

    pub fn fill_screen(&mut self) {
        self.painter.paint_all(BACKGROUND_COLOR);
    }
}

static OUTPUT_COLOR: Color = Color::new(0, 0, 0); // Black
pub static BACKGROUND_COLOR: Color = Color::new(255, 255, 255); // White

impl<'fb> core::fmt::Write for Console<'fb> {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        let mut x = self.x;
        let mut y = self.y;
        for c in s.chars() {
            // Scroll and repaint before putting char,
            // if cursor reaches the bottom of the console.
            if y == ROWS {
                self.painter.paint_all(BACKGROUND_COLOR);
                for r in 0..ROWS - 1 {
                    self.buf[r] = self.buf[r + 1];
                    for (c, &ch) in self.buf[r].iter().enumerate() {
                        if ch == '\n' {
                            break;
                        }
                        let pos = position(c, r);
                        self.painter.paint_ascii(ch, pos.0, pos.1, OUTPUT_COLOR);
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
                let pos = position(x, y);
                self.painter.paint_ascii(c, pos.0, pos.1, OUTPUT_COLOR);
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

#[inline]
fn position(x: usize, y: usize) -> (usize, usize) {
    (X_PAD + x * FW, Y_PAD + y * FH)
}

macro_rules! kprint {
    ($($arg:tt)*) => {{
        use core::fmt::Write as _;
        use crate::services::CONSOLE as _CONSOLE;
        let console = unsafe { _CONSOLE.as_mut().unwrap() };
        write!(console, $($arg)*).expect("printk failed.");
    }};
}

macro_rules! kprintln {
    () => {{
        kprint!("\n");
    }};

    ($fmt:expr, $($arg:tt)*) => {{
        kprint!(concat!($fmt, "\n"), $($arg)*);
    }};
}
