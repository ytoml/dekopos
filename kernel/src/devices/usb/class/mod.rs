use self::hid::keyboard::Keyboard;
use self::hid::mouse::Mouse;
use self::hid::Hid;
use super::data_types::{EndpointConfig, EndpointId};
use super::mem::{Box, UsbAllocator};
use super::Result;
use crate::devices::usb::data_types::InterfaceDescriptor;

pub(super) const HID_BUFSIZE: usize = 1024;

pub trait ClassDriver {
    fn set_endpoints(&mut self, configs: &[EndpointConfig]);
    fn on_control_completed(&mut self) -> Result<()>;
    fn on_endpoints_configured(&mut self) -> Result<()>;
    fn on_interrupt_completed(&mut self, id: EndpointId) -> Result<()>;
}
impl core::fmt::Debug for dyn ClassDriver {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "ClassDriver")
    }
}
trait OnDataReceived {
    const BUFSIZE: usize;
    fn on_data_received(&mut self, buf: &[u8]);
}

pub fn new_class_driver(if_desc: InterfaceDescriptor) -> Box<dyn ClassDriver> {
    // TODO: better interface to register observer functions?
    if if_desc.interface_class() == 3 && if_desc.interface_sub_class() == 1
    // HID boot interface
    {
        match if_desc.interface_protocol() {
            1 => {
                let body = Keyboard::new(crate::key_push);
                let hid = Hid::new(if_desc.interface_number().into(), body);
                return Box::new_in(hid, UsbAllocator) as Box<dyn ClassDriver>;
            }
            2 => {
                let body = Mouse::new(crate::mouse_move);
                let hid = Hid::new(if_desc.interface_number().into(), body);
                return Box::new_in(hid, UsbAllocator) as Box<dyn ClassDriver>;
            }
            _ => {}
        }
    }
    panic!("Unsupported interface.");
}

mod hid;
