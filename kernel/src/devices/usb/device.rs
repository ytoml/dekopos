use core::ptr;

use heapless::LinearMap;
use xhci::ring::trb::event::TransferEvent;
use xhci::ring::trb::transfer::SetupStage;

use super::class::ClassDriver;
use super::context::DeviceCtx;
use super::driver::Driver;
use super::mem::{Box, Vec, XhcAllocator};
use super::ring::{self, TransferRing, TrbT};
use super::{Error, Result, NUM_OF_ENDPOINTS};

const N_EVENT_WAITERS: usize = 4;

#[derive(Debug)]
pub(super) struct Manager {
    devices: Vec<Device>,
}

#[derive(Debug)]
pub(super) struct Device {
    tr: TransferRing,
    ctx: DeviceCtx,
    driver: Driver,
}

/// Represents next to do with.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u8)]
enum State {
    InitializeUninvoked = 0,
    GetDeviceDesc = 1,
    GetConfigDesc = 2,
    SetConfig = 3,
    Ready = 4, // initialized
}

impl Device {
    pub fn new(ctx: DeviceCtx, tr: TransferRing, driver: Driver) -> Self {
        Self { tr, ctx, driver }
    }

    pub(super) fn transfer_event(&mut self, te: TransferEvent) -> Result<()> {
        use xhci::ring::trb::event::CompletionCode;
        let residual_len = te.trb_transfer_length();
        match te.completion_code() {
            Ok(CompletionCode::Success) | Ok(CompletionCode::ShortPacket) => {}
            code => {
                log::debug!("Device::transfer_event: Invalid command completion code {code:?}")
            }
        }
        match unsafe { ring::read_trb(te.trb_pointer()) } {
            Err(bytes) => Err(Error::UnexpectedTrbContent(bytes)),
            Ok(trb) => match trb {
                TrbT::Normal(normal) => todo!(),
                TrbT::DataStage(ds) => todo!(),
                _ => Err(Error::InvalidPortPhase),
            },
        }
    }
}
