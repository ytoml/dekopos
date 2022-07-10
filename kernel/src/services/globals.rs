use super::logging;
use crate::devices;
use crate::devices::pci::PciDeviceService;
use crate::graphics::console::Console;
use crate::graphics::FrameBuffer;

// Place FrameBuffer in global field is valid because FrameBuffer itself
// does not contains the content of frame buffer and we can assume that
// the exact location of frame buffer does not change from its original
// even if we move the location of FrameBuffer.
static mut FRAME_BUFFER: Option<FrameBuffer> = None;
pub(super) static mut CONSOLE: Option<Console> = None;
static mut PCI_DEVICES: PciDeviceService = PciDeviceService::new();
static mut MMAP: Option<::common_data::mmap::MemMap> = None;

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

    // interrupt handler
    devices::setup_handler();
}

access_static_as_mut_unwrap!(pub console_mut, CONSOLE, Console<'static>);
access_static_as_ref_unwrap!(pub mmap, MMAP, ::common_data::mmap::MemMap);
access_static_mut!(pub pci_devices_service_mut, PCI_DEVICES, PciDeviceService);
