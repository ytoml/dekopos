use xhci::context::EndpointType;
use xhci::ring::trb::transfer::{Direction, SetupStage, TransferType};

use self::hid::Hid;
use self::hid::mouse::Mouse;
use self::hid::keyboard::Keyboard;
use super::data_types::{EndpointConfig, EndpointId, EndpointIndex, Recipient, RequestType, Type};
use super::device::Device;
use super::{Error, Result};
use super::mem::{Box, XhcAllocator};

pub(super) const HID_BUFSIZE: usize = 1024;

pub(super) trait ClassDriver {
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
    Keyboard(fn(modifier: u8, keycode:u8, press: bool))
}
impl Supported {
    pub(super) fn build(self, interface_index: EndpointIndex) -> Box<dyn ClassDriver> {
        match self {
            Self::Mouse(observer) => Box::new_in(Hid::new(interface_index, Mouse::new(observer)), XhcAllocator) as Box<dyn ClassDriver>, 
            Self::Keyboard(observer) => Box::new_in(Hid::new(interface_index, Keyboard::new(observer)), XhcAllocator) as Box<dyn ClassDriver>,
        }
    }
}

mod hid;
