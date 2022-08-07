use self::hid::keyboard::Keyboard;
use self::hid::mouse::Mouse;
use self::hid::Hid;
use super::data_types::{EndpointConfig, EndpointId, EndpointIndex};
use super::mem::{Box, UsbAllocator};
use super::Result;

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

#[derive(Debug, Clone, Copy)]
pub enum Supported {
    Mouse(fn(button: u8, dx: u8, dy: u8)),
    Keyboard(fn(modifier: u8, keycode: u8, press: bool)),
}
impl Supported {
    pub(super) fn build(self, interface_index: EndpointIndex) -> Box<dyn ClassDriver> {
        match self {
            Self::Mouse(observer) => Box::new_in(
                Hid::new(interface_index, Mouse::new(observer)),
                UsbAllocator,
            ) as Box<dyn ClassDriver>,
            Self::Keyboard(observer) => Box::new_in(
                Hid::new(interface_index, Keyboard::new(observer)),
                UsbAllocator,
            ) as Box<dyn ClassDriver>,
        }
    }
}

mod hid;
