use core::ptr;

use heapless::LinearMap;
use xhci::ring::trb::event::TransferEvent;
use xhci::ring::trb::transfer::SetupStage;

use super::class::ClassDriver;
use super::context::DeviceCtx;
use super::data_types::{EndpointId, SetupData};
use super::mem::{Box, Vec, XhcAllocator};
use super::{Error, Result, NUM_OF_ENDPOINTS};

const N_EVENT_WAITERS: usize = 4;

#[derive(Debug)]
pub(super) struct Driver {
    state: State,
    // TODO: how to manage pointers to dynamic trait objects ?
    class_drivers: Vec<Option<Box<dyn ClassDriver>>>,
    event_waiters: LinearMap<SetupData, Box<dyn ClassDriver>, { N_EVENT_WAITERS }>,
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

impl Driver {
    pub fn new() -> Self {
        let mut class_drivers = Vec::with_capacity_in(NUM_OF_ENDPOINTS, XhcAllocator);
        for _ in 0..N_EVENT_WAITERS {
            class_drivers.push(None);
        }

        Self {
            state: State::InitializeUninvoked,
            class_drivers,
            event_waiters: LinearMap::new(),
        }
    }

    pub fn invoke_init(&mut self) {
        self.state = State::GetDeviceDesc;
        todo!() // get descriptor
    }

    pub(super) fn interrupt_in(&self) -> Result<()> {
        Ok(())
    }
    pub(super) fn interrupt_out(&self) -> Result<()> {
        Ok(())
    }

    pub(super) fn control_in(
        &mut self,
        setup: SetupData,
        issuer: Box<dyn ClassDriver>,
    ) -> Result<()> {
        self.register_event_waiter(setup, issuer)
    }

    pub(super) fn control_out(
        &mut self,
        setup: SetupData,
        issuer: Box<dyn ClassDriver>,
    ) -> Result<()> {
        self.register_event_waiter(setup, issuer)
    }

    fn register_event_waiter(
        &mut self,
        setup: SetupData,
        issuer: Box<dyn ClassDriver>,
    ) -> Result<()> {
        if self
            .event_waiters
            .insert(setup, issuer)
            .map_err(|_| Error::EventWaitersFull)?
            .is_some()
        {
            log::debug!("devices::usb::driver::Driver::regiter_event_waiter: Implicitly updated class driver on {setup:?}")
        }
        Ok(())
    }

    pub fn on_control_completed(&self, id: EndpointId, data: SetupStage, buf: &[u8]) -> Result<()> {
        // TODO: interpret descriptor
        match self.state {
            State::GetDeviceDesc => {
                todo!()
            }
            State::GetConfigDesc => {
                todo!()
            }
            State::SetConfig => {
                todo!()
            }
            State::Ready => {
                todo!()
            }
            State::InitializeUninvoked => Err(Error::InvalidDeviceInitializationState(
                "devices::usb::device::Device::on_control_completed",
            )),
        }
    }

    pub fn on_endpoints_configured(&mut self) -> Result<()> {
        for driver in self.class_drivers.iter_mut() {
            if let Some(driver) = driver.as_mut() {
                driver.on_endpoints_configured()?;
            }
        }
        Ok(())
    }

    pub fn on_interrupt_completed(&mut self, id: EndpointId) -> Result<()> {
        self.class_drivers[id.value()]
            .as_mut()
            .ok_or_else(|| Error::ClassDriverNotFound(id.value()))?
            .on_interrupt_completed(id)
    }
}
