extern crate alloc;

use xhci::context::EndpointType;
use xhci::ring::trb::transfer::{Direction, SetupStage, TransferType};

use super::{ClassDriver, OnDataReceived};
use crate::devices::usb::data_types::{
    EndpointConfig, EndpointId, EndpointIndex, Recipient, RequestType, Type,
};
use crate::devices::usb::mem::{UsbAllocator, Vec};
use crate::devices::usb::{Error, Result};

pub(super) const HID_BUFSIZE: usize = 1024;

auto_repr_tryfrom! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
    pub(super) enum HidRequest: u8 {
        GetReport = 1,
        GetProtocol = 11,
    }
}
auto_repr_tryfrom! {
    /// Init phase
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
    pub enum Phase: u8 {
        UnAddressed = 0,
        Stage1 = 1,
        Stage2 = 2,
        Stage3 = 3,
    }
}
impl Default for Phase {
    fn default() -> Self {
        Self::UnAddressed
    }
}

#[derive(Debug)]
pub(super) struct Hid<Impl: OnDataReceived> {
    buf: Vec<u8>,
    if_index: EndpointIndex,
    init_phase: Phase,
    int_in: EndpointId,
    int_out: EndpointId,
    body: Impl,
}

impl<Impl: OnDataReceived> Hid<Impl> {
    pub fn new(if_index: EndpointIndex, body: Impl) -> Self {
        Self {
            buf: vec_no_realloc![0u8; Impl::BUFSIZE; UsbAllocator],
            if_index,
            init_phase: Phase::UnAddressed,
            int_in: EndpointId::zeroed(),
            int_out: EndpointId::zeroed(),
            body,
        }
    }

    fn init(&self) -> Result<()> {
        Err(Error::Unimplemented("devices::usb::class::hid::Hid::init"))
    }
}
impl<Impl: OnDataReceived> ClassDriver for Hid<Impl> {
    fn set_endpoints(&mut self, configs: &[EndpointConfig]) {
        for config in configs.iter() {
            match config.ty {
                EndpointType::InterruptIn => {
                    self.int_in = config.id;
                }
                EndpointType::InterruptOut => {
                    self.int_out = config.id;
                }
                _ => {}
            }
        }
    }

    fn on_control_completed(&mut self) -> Result<()> {
        match self.init_phase {
            Phase::Stage1 => {
                self.init_phase = Phase::Stage2;
                Ok(())
            }
            _ => Err(Error::InvalidHidPhase(
                "<devices::usb::class::hid::Hid as ClassDriver>::on_control_completed",
            )),
        }
    }

    fn on_endpoints_configured(&mut self) -> Result<()> {
        let mut setup = SetupStage::new();
        setup
            .set_request_type(
                RequestType::from((Recipient::Interface, Type::Class, Direction::Out)).into(),
            )
            .set_request(HidRequest::GetProtocol.into())
            .set_length(0)
            // boot protocol
            .set_value(0)
            .set_index(self.if_index.into())
            .set_transfer_type(TransferType::No);
        self.init_phase = Phase::Stage1;
        Ok(())
    }

    fn on_interrupt_completed(&mut self, id: EndpointId) -> Result<()> {
        match id.direction() {
            Direction::In => {
                self.body.on_data_received(&self.buf);
                Ok(())
            }
            Direction::Out => Err(Error::Unimplemented(
                "devices::usb::class::Hid::on_interrupt_completed",
            )),
        }
    }
}

pub mod keyboard;
pub mod mouse;
