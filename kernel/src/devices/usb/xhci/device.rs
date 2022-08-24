use core::pin::Pin;

use xhci::context::{DeviceHandler, InputHandler, NUM_OF_ENDPOINT_CONTEXTS};
use xhci::ring::trb::transfer::{DataStage, Direction, SetupStage, StatusStage, TransferType};

use super::context::{DeviceCtx, InputCtx};
use super::ring::{TransferRing, TrbT};
use super::usb::class::ClassDriver;
use super::usb::data_types::SetupData;
use super::usb::data_types::{DeviceContextIndex, EndpointId};
use super::usb::mem::{BoundedBox64, Box, UsbAllocator, Vec};
use super::usb::utils;
use super::usb::{Error, Result, TR_SIZE};
use crate::devices::usb::NUM_OF_ENDPOINTS;

const N_EP_CTX: usize = 31;

#[derive(Debug)]
pub struct Device {
    // tr[i] corresponds to device ctx pointed from DBCAA[i+1]
    // This implementation assumes streams are not used.
    // See 4.12.2 of xHCI specification.
    transfer_rings: Vec<Option<TransferRing>>,
    dev_ctx: Pin<BoundedBox64<DeviceCtx>>,
    inp_ctx: Pin<BoundedBox64<InputCtx>>,

    // According to https://github.com/uchan-nos/mikanos/blob/c1a734f594bceb0767fe630b0b2cd3fef227bf16/kernel/usb/device.cpp and other implementations,
    // maybe we don't need map here (actually map of setupdata -> class driver is unused).
    class_drivers: Vec<Option<Box<dyn ClassDriver>>>,
}

impl Device {
    pub fn new(
        dev_ctx: Pin<BoundedBox64<DeviceCtx>>,
        inp_ctx: Pin<BoundedBox64<InputCtx>>,
    ) -> Self {
        Self {
            transfer_rings: vec_no_realloc_none![NUM_OF_ENDPOINT_CONTEXTS; UsbAllocator],
            class_drivers: vec_no_realloc_none![NUM_OF_ENDPOINTS; UsbAllocator],
            dev_ctx,
            inp_ctx,
        }
    }

    pub fn new_with_ep0_enabled(
        dev_ctx: Pin<BoundedBox64<DeviceCtx>>,
        inp_ctx: Pin<BoundedBox64<InputCtx>>,
    ) -> Self {
        let mut device = Self::new(dev_ctx, inp_ctx);
        device
            .new_transfer_ring_at(DeviceContextIndex::EP0)
            .unwrap();
        device
    }

    pub fn get_mut_transfer_ring_at(
        &mut self,
        dci: DeviceContextIndex,
    ) -> Result<&mut TransferRing> {
        let index = dci.as_index_from_zero();
        self.transfer_rings[index]
            .as_mut()
            .ok_or(Error::TransferRingNotAllocatedForDevice)
    }

    pub fn input_context_mut(&mut self) -> &mut InputCtx {
        &mut self.inp_ctx
    }

    pub fn input_context(&self) -> &InputCtx {
        &self.inp_ctx
    }

    pub fn device_context_mut(&mut self) -> &mut DeviceCtx {
        &mut self.dev_ctx
    }

    pub fn device_context(&self) -> &DeviceCtx {
        &self.dev_ctx
    }

    pub fn get_root_hub_port_number(&self) -> u8 {
        self.dev_ctx.slot().root_hub_port_number()
    }
}

impl Device {
    pub fn new_transfer_ring_at(&mut self, dci: DeviceContextIndex) -> Result<()> {
        let index = dci.as_index_from_zero();
        if self.transfer_rings[index].is_some() {
            Err(Error::TransferRingDuplicatedForSameDci)
        } else {
            let tr = TransferRing::new(TR_SIZE);

            let dci = dci.into_raw();
            let port_speed_value = self.inp_ctx.device_mut().slot_mut().speed();
            let max_packet_size = utils::get_max_packet_size(port_speed_value);

            self.inp_ctx.control_mut().set_add_context_flag(dci);
            let mut inp_pin = self.inp_ctx.as_mut();
            let ep = inp_pin.device_mut().endpoint_mut(dci);
            ep.set_max_packet_size(max_packet_size);
            if tr.producer_cycle_state() {
                ep.set_dequeue_cycle_state();
            }
            ep.set_tr_dequeue_pointer(tr.head_addr());

            let _ = self.transfer_rings[index].insert(tr);
            Ok(())
        }
    }
}

impl Device {
    /// Direction is decided according to provided [`EndpointId`].
    /// Caller must guarantee [`buf`] (if provided) alive until
    /// when device actually writes data to it
    /// or data will be lost.
    pub fn control_transfer(
        &mut self,
        id: EndpointId,
        setup: SetupData,
        // Requires heap allocated buffer because buffer on stack
        // might be relocated even if it is pinned when caller function returns.
        // That means, device might write data to memory where is unused anymore.
        buf: Option<&mut Pin<Box<[u8]>>>,
    ) -> Result<u64> {
        let dci = DeviceContextIndex::try_from(id).unwrap();
        let tr = self.get_mut_transfer_ring_at(dci)?;

        let mut status = StatusStage::new();
        let mut setup: SetupStage = setup.into();
        if let Some(buf) = buf {
            let buflen = buf.len();
            let mut data = *DataStage::new().set_interrupt_on_completion();
            // PR to xhci crate?
            if !(1..=64 * 1024).contains(&buflen) {
                return Err(Error::InvalidTransferLength(buflen));
            } else {
                data.set_data_buffer_pointer(buf.as_ptr() as u64)
                    .set_trb_transfer_length(buflen.try_into().unwrap())
                    .set_td_size(0)
                    .set_direction(id.direction());
            }
            match id.direction() {
                Direction::In => {
                    setup.set_transfer_type(TransferType::In);
                }
                Direction::Out => {
                    setup.set_transfer_type(TransferType::Out);
                }
            }
            let _ = tr.push(TrbT::SetupStage(setup));
            let data_addr = tr.push(TrbT::DataStage(data));
            let _ = tr.push(TrbT::StatusStage(status));

            Ok(data_addr)
        } else {
            status.set_interrupt_on_completion();
            setup.set_transfer_type(TransferType::No);
            if id.direction() == Direction::In {
                status.set_direction();
            }
            let _ = tr.push(TrbT::SetupStage(setup));
            let status_addr = tr.push(TrbT::StatusStage(status));

            Ok(status_addr)
        }
    }

    pub fn on_interrupt_completed(&mut self, id: EndpointId) -> Result<()> {
        self.class_drivers[id.as_index()]
            .as_mut()
            .ok_or(Error::ClassDriverNotAllocatedForEndpoint(id.as_index()))?
            .on_interrupt_completed(id)
    }

    pub fn on_endpoints_configured(&mut self) -> Result<()> {
        for driver in self.class_drivers.iter_mut().filter_map(|op| op.as_mut()) {
            driver.on_endpoints_configured()?;
        }
        Ok(())
    }

    pub fn set_endpoints(&mut self) -> Result<()> {
        for driver in self.class_drivers.iter_mut() {
            if let Some(_driver) = driver.as_mut() {
                // TODO: manage endpoint configs
                // driver.set_endpoints(configs)
            }
        }
        Ok(())
    }
}
