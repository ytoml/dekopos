use crate::graphics::FrameBuffer;
use crate::graphics::console::Console;

// Place FrameBuffer in global field is valid because FrameBuffer itself
// does not contains the content of frame buffer and we can assume that
// the exact location of frame buffer does not change from its original
// even if we move the location of FrameBuffer.
pub(crate) static mut FRAME_BUFFER: Option<FrameBuffer> = None;
pub(crate) static mut CONSOLE: Option<Console> = None;

/// This function is expected to be called at the very start of the entry of the kernel.
pub fn init(fb: *mut ::common_data::graphics::FrameBuffer) {
    unsafe {
        let _ = FRAME_BUFFER.insert(fb.read().into()); 
        let mut console = Console::from_frame_buffer(FRAME_BUFFER.as_mut().unwrap());
        console.fill_screen();
        let _ = CONSOLE.insert(console);
    }
}