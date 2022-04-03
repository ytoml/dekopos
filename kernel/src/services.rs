use crate::devices::pci::PciDeviceService;
use crate::graphics::console::Console;
use crate::graphics::{Color, FrameBuffer};

// Place FrameBuffer in global field is valid because FrameBuffer itself
// does not contains the content of frame buffer and we can assume that
// the exact location of frame buffer does not change from its original
// even if we move the location of FrameBuffer.
pub(crate) static mut FRAME_BUFFER: Option<FrameBuffer> = None;
pub(crate) static mut CONSOLE: Option<Console> = None;
// Ugly hack?:
// When declaring this static variable as PciDeviceService (its constructor is const fn),
// memory location goes wrong and device service occurs unexpected panic (with destroyed self.count).
// However, when set it at runtime (i.e. inside init() below), it seems to work well.
pub(crate) static mut PCI_DEVICES: Option<PciDeviceService> = None;

/// This function is expected to be called at the very start of the entry of the kernel.
pub fn init(fb: *mut ::common_data::graphics::FrameBuffer) {
    unsafe {
        // screen services
        let _ = FRAME_BUFFER.insert(fb.read().into());
        let mut console = Console::from_frame_buffer(FRAME_BUFFER.as_mut().unwrap());
        console.set_background_color(Color::BLUE);
        console.set_output_color(Color::WHITE);
        console.fill_screen();
        let _ = CONSOLE.insert(console);

        // device services
        let _ = PCI_DEVICES.insert(PciDeviceService::new());
    }
}
