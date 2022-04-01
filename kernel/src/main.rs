#![no_std]
#![no_main]

use core::arch::asm;
use core::panic::PanicInfo;

mod graphic;

use graphic::{Color, FrameBuffer};

#[no_mangle]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "sysv64" fn kernel_main(fb: *mut ::common_data::graphic::FrameBuffer) {
    let mut fb: FrameBuffer = unsafe { fb.read() }.into();
    render_mushroom(&mut fb);

    loop {
        unsafe {
            asm!("hlt");
        }
    }
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {
        unsafe {
            asm!("hlt");
        }
    }
}

fn render_mushroom(fb: &mut FrameBuffer) {
    const WHITE: Color = Color::new(255, 255, 255);
    const RED: Color = Color::new(255, 0, 0);
    const CREAM: Color = Color::new(255, 237, 179);
    const BLACK: Color = Color::new(0, 0, 0);
    let (w, h) = fb.resolution();
    let mut p = fb.painter();

    let mut write = |w, h, o: (usize, usize), c| {
        for x in 0..w {
            for y in 0..h {
                p.paint(x + o.0, y + o.1, c);
            }
        }
    };

    // Write red mushroom on screen.
    write(w, h, (0, 0), WHITE);
    write(200, 100, (100, 100), RED);
    write(100, 100, (150, 150), CREAM);
    write(15, 15, (170, 190), BLACK);
    write(15, 15, (215, 190), BLACK);
    write(10, 10, (110, 120), WHITE);
    write(10, 10, (150, 130), WHITE);
    write(10, 10, (180, 130), WHITE);
    write(10, 10, (220, 110), WHITE);
    write(10, 10, (250, 120), WHITE);
    write(10, 10, (280, 110), WHITE);
}
