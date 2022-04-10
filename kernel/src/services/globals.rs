use super::logging;
use crate::devices::pci::PciDeviceService;
use crate::graphics::console::Console;
use crate::graphics::FrameBuffer;

// Place FrameBuffer in global field is valid because FrameBuffer itself
// does not contains the content of frame buffer and we can assume that
// the exact location of frame buffer does not change from its original
// even if we move the location of FrameBuffer.
pub(crate) static mut FRAME_BUFFER: Option<FrameBuffer> = None;
pub(crate) static mut CONSOLE: Option<Console> = None;
pub(crate) static mut PCI_DEVICES: PciDeviceService = PciDeviceService::new();
pub(crate) static mut MMAP: Option<::common_data::mmap::MemMap> = None;

/// # Safety
/// This function is expected to be called at the very start of the entry of the kernel.
/// Do not use this twice.
pub unsafe fn init(
    mmap: *const ::common_data::mmap::MemMap,
    fb: *mut ::common_data::graphics::FrameBuffer,
) {
    // screen services
    let _ = FRAME_BUFFER.insert(fb.read().into());
    let console = Console::from_frame_buffer(FRAME_BUFFER.as_mut().unwrap());
    logging::logger_init(console);

    // memory map
    let _ = MMAP.insert(mmap.read());
}
