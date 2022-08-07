extern crate alloc;

use core::pin::Pin;

use heapless::LinearMap;
use xhci::registers::doorbell::Register as DoorbellRegister;
use xhci::ring::trb::command::AddressDevice;
use xhci::ring::trb::event::TransferEvent;
use xhci::ring::trb::transfer::{DataStage, Direction, SetupStage};

use super::class::ClassDriver;
use super::data_types::{
    ConfigDescReader, DescriptorType, DeviceContextIndex, EndpointId, Recipient, RequestCode,
    SetupData, Supported, Type,
};
use super::mem::{BoundedAlloc64, Box, ReadWriteArray, UsbAllocator, Vec};
use super::utils;
use super::xhci::context::{DeviceContextBaseAddressArray, DeviceCtx, InputCtx};
use super::xhci::device::Device;
use super::xhci::ring::{TrbE, TrbT};
use super::{Error, Result, NUM_OF_ENDPOINTS};
use xhci::context::InputHandler;

const N_EVENT_WAITERS: usize = 4;

#[derive(Debug)]
pub(super) struct DeviceManager {
    drivers: Vec<Option<Driver>>,
    dcbaa: DeviceContextBaseAddressArray,
    // doorbells for each device.
    // Note that doorbells[i] is for slotid i+1.
    doorbells: ReadWriteArray<DoorbellRegister>,

    // Page boundary used to appropriately allocate device contexts.
    ctx_alloc: BoundedAlloc64,
}
impl DeviceManager {
    pub fn new(doorbells: ReadWriteArray<DoorbellRegister>, page_boundary: u64) -> Self {
        let max_slots_enabled = doorbells.len();
        assert!(max_slots_enabled > 0, "At least 1 slot must be allocated.");

        // head of dcbaa unused
        let capacity = max_slots_enabled + 1;
        log::info!("DCBAA capacity: {:?}", capacity);
        let dcbaa = DeviceContextBaseAddressArray::new(capacity);
        let mut drivers = Vec::with_capacity_in(capacity, UsbAllocator);
        for _ in 0..capacity {
            drivers.push(None);
        }
        // let devices = Vec::new_in(UsbAllocator);

        Self {
            drivers,
            dcbaa,
            doorbells,
            ctx_alloc: BoundedAlloc64::new(page_boundary),
        }
    }

    pub fn enable_at(&mut self, slot_id: u8, port_id: u8, port_speed: u8) -> Result<AddressDevice> {
        let i: usize = slot_id.into();
        if self.drivers[i].is_some() {
            Err(Error::DeviceAlreadyAllocatedForSlot(slot_id))
        } else {
            let dev_ctx = DeviceCtx::new_32byte();
            let (dev_ctx, dev_ctx_ptr_raw) = utils::leak_raw_pin(dev_ctx, self.ctx_alloc);

            let mut inp_ctx = InputCtx::new_32byte();
            inp_ctx.control_mut().set_add_context_flag(0); // slot
            {
                let slot = inp_ctx.device_mut().slot_mut();
                slot.set_root_hub_port_number(port_id);
                slot.set_speed(port_speed);
            }
            let (inp_ctx, inp_ctx_ptr_raw) = utils::leak_raw_pin(inp_ctx, self.ctx_alloc);

            let mut device = Device::new(dev_ctx, inp_ctx);
            device.new_transfer_ring_at(DeviceContextIndex::EP0)?;
            let driver = Driver::new(device);

            self.dcbaa.register(slot_id, dev_ctx_ptr_raw);
            let _ = self.drivers[i].insert(driver);

            Ok(*AddressDevice::new()
                .set_input_context_pointer(inp_ctx_ptr_raw as u64)
                .set_slot_id(slot_id))
        }
    }

    pub fn update_doorbell_at<U>(&mut self, slot_id: u8, f: U)
    where
        U: FnOnce(&mut DoorbellRegister),
    {
        assert!(
            slot_id > 0,
            "doorbell 0 is for host controller and DeviceManager doesn't provide access to it."
        );
        self.doorbells.update_volatile_at((slot_id - 1) as usize, f);
    }

    pub fn initialize_at(&mut self, slot_id: u8) -> Result<()> {
        self.get_mut_at(slot_id)?.invoke_init()
    }

    pub fn get_port_of_device_at(&self, slot_id: u8) -> Result<u8> {
        self.get_device_at(slot_id)
            .map(|d| d.get_root_hub_port_number())
    }

    pub fn dcbaa_pointer(&self) -> u64 {
        self.dcbaa.head_addr()
    }

    fn get_mut_at(&mut self, slot_id: u8) -> Result<&mut Driver> {
        self.drivers[slot_id as usize]
            .as_mut()
            .ok_or(Error::DeviceNotAllocatedForSlot(slot_id))
    }

    fn get_mut_device_at(&mut self, slot_id: u8) -> Result<&mut Device> {
        self.get_mut_at(slot_id).map(|d| &mut d.device)
    }

    fn get_at(&self, slot_id: u8) -> Result<&Driver> {
        self.drivers[slot_id as usize]
            .as_ref()
            .ok_or(Error::DeviceNotAllocatedForSlot(slot_id))
    }

    fn get_device_at(&self, slot_id: u8) -> Result<&Device> {
        self.get_at(slot_id).map(|d| &d.device)
    }

    pub fn on_event(&mut self, _event: TrbE) -> Result<()> {
        todo!()
    }
}

/// Provides operation per device
#[derive(Debug)]
pub(super) struct Driver {
    device: Device,
    state: State,

    // event_waiters: LinearMap<SetupData, usize, { N_EVENT_WAITERS }>,
    // event_waiters_inv: Vec<Option<SetupData>>,

    // issuer(phys addr) -> TRB
    // maybe we need this (rather than Set) because Issuer itself does not guarantee uniqueness.
    issuers: LinearMap<u64, Issuer, 16>,

    // descriptor buffer per endpoint (in direction)
    desc_bufs: Vec<Option<Pin<Box<[u8]>>>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum State {
    Uninvoked,
    Initializing(Next),
    Ready,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Next {
    GetConfigDesc,
    ReadSetConfig,
    SetEndpoint,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Issuer {
    DataStage(DataStage),
    SetupStage(SetupStage),
}

impl Driver {
    pub fn new(device: Device) -> Self {
        Self {
            device,
            state: State::Uninvoked,
            // event_waiters: LinearMap::new(),
            // event_waiters_inv,
            issuers: LinearMap::new(),
            desc_bufs: vec_no_realloc_none![NUM_OF_ENDPOINTS; UsbAllocator],
        }
    }

    pub fn invoke_init(&mut self) -> Result<()> {
        self.state = State::Initializing(Next::GetConfigDesc);
        let buf = self.get_device_desc(EndpointId::DEFAULT_CONTROL)?;
        let _ = self.desc_bufs[EndpointId::DEFAULT_CONTROL.as_index()].insert(buf);
        Ok(())
    }

    /// Returned buffer is where device descriptor will be written
    pub fn get_device_desc(&mut self, id: EndpointId) -> Result<Pin<Box<[u8]>>> {
        // 18 bytes of array required, but just in case larger allocation
        let mut buf = Pin::new(vec_no_realloc![0u8; 32; UsbAllocator].into_boxed_slice());
        self.get_descriptor(id, DescriptorType::Device, 0, &mut buf)?;
        Ok(buf)
    }

    pub(super) fn interrupt_in(&self) -> Result<()> {
        Ok(())
    }
    pub(super) fn interrupt_out(&self) -> Result<()> {
        Ok(())
    }

    fn register_event_waiter(
        &mut self,
        _setup: SetupData,
        _issuer: Box<dyn ClassDriver>,
    ) -> Result<()> {
        Ok(())
    }

    fn validate_desc_buf(&self, id: EndpointId, found_addr: u64) -> Result<()> {
        let expected_addr = self.desc_bufs[id.as_index()]
            .as_ref()
            .ok_or(Error::DescriptorBufferNotAllocated)?
            .as_ptr() as u64;
        if expected_addr == found_addr {
            Ok(())
        } else {
            Err(Error::DescriptorLost {
                expected_addr,
                found_addr,
            })
        }
    }

    pub fn transfer_event(&mut self, te: TransferEvent) -> Result<()> {
        use xhci::ring::trb::event::CompletionCode;
        match te.completion_code() {
            Ok(CompletionCode::Success) | Ok(CompletionCode::ShortPacket) => {}
            code => {
                log::debug!("Device::transfer_event: Invalid command completion code {code:?}")
            }
        }
        let issuer_ptr = te.trb_pointer();
        let _residual_len = te.trb_transfer_length(); // specify length
        let id: EndpointId = te
            .endpoint_id()
            .try_into()
            .expect("invalid endpoint id found, something is wrong with device or mmio mapping.");
        match unsafe { super::xhci::ring::read_trb(issuer_ptr) } {
            Err(bytes) => Err(Error::UnexpectedTrbContent(bytes)),
            Ok(trb) => match trb {
                TrbT::Normal(_normal) => {
                    self.device.on_interrupt_completed(id)?;
                    todo!()
                }
                TrbT::DataStage(ds) => {
                    self.validate_desc_buf(id, ds.data_buffer_pointer())?;
                    todo!()
                }
                TrbT::SetupStage(setup) => {
                    let issuer = self
                        .issuers
                        .get(&issuer_ptr)
                        .ok_or(Error::NoCorrespondingIssuerTrb(issuer_ptr))?;
                    match issuer {
                        Issuer::DataStage(_) => return Err(Error::TrbAddressConflicts(issuer_ptr)),
                        Issuer::SetupStage(setup_registered) => {
                            debug_assert_eq!(&setup, setup_registered, "Setup Stage TRB changed from when issued. Maybe something wrong with devices.");
                            let _setup: SetupData =
                                setup.try_into().map_err(Error::InvalidSetupStageTrb)?;

                            todo!()
                        }
                    }
                    todo!()
                }
                _ => Err(Error::InvalidPortPhase),
            },
        }
    }

    pub fn on_control_completed(
        &mut self,
        _id: EndpointId,
        _data: SetupStage,
        _desc_buf: &[u8],
    ) -> Result<()> {
        // TODO: interpret descriptor
        match self.state {
            State::Ready => Ok(()),
            State::Initializing(next) => match next {
                Next::GetConfigDesc => self.get_config_desc(),
                Next::ReadSetConfig => {
                    todo!()
                }
                Next::SetEndpoint => {
                    todo!()
                }
            },
            State::Uninvoked => Err(Error::InvalidDeviceInitializationState(
                "devices::usb::device::Device::on_control_completed",
            )),
        }
    }

    fn get_config_desc(&self) -> Result<()> {
        Err(Error::Unimplemented("TODO"))
    }

    fn read_and_set_config(&mut self, buf: &[u8]) -> Result<()> {
        let mut reader = ConfigDescReader::new(buf);
        while let Some(sup) = reader.next() {
            let if_desc = match sup? {
                Supported::Interface(desc) => desc,
                _ => return Err(Error::InvalidlyOrderedDescriptorFound),
            };
            for _ in 0..if_desc.num_endpoints() {
                let desc = reader.next().expect("number of descriptors conflicts.")?;
                match desc {
                    Supported::Interface(_) => return Err(Error::InvalidlyOrderedDescriptorFound),
                    Supported::Endpoint(_desc) => {
                        // TODO: make_ep_config()
                        todo!()
                    }
                    Supported::Hid(desc) => {
                        log::debug!("{desc:?}");
                    }
                }
            }
        }
        self.state = State::Initializing(Next::SetEndpoint);
        Ok(())
    }

    fn set_endpoints(&mut self) -> Result<()> {
        self.device.set_endpoints()?;
        self.state = State::Ready;
        Ok(())
    }

    fn control_transfer(
        &mut self,
        id: EndpointId,
        setup: SetupData,
        buf: Option<&mut Pin<Box<[u8]>>>,
    ) -> Result<()> {
        let issuer_trb_addr = self.device.control_transfer(id, setup, buf)?;
        // Not handle returned Option because it's possible same address
        // used in next ring cycle.
        let _ = self
            .issuers
            .insert(issuer_trb_addr, Issuer::SetupStage(setup.into()))
            .map_err(|_| Error::TrbIssuerMapFull)?;
        Ok(())
    }

    fn get_descriptor(
        &mut self,
        id: EndpointId,
        desc_type: DescriptorType,
        desc_index: u8,
        buf: &mut Pin<Box<[u8]>>,
    ) -> Result<()> {
        let desc_type: u8 = desc_type.into();
        let setup = SetupData {
            request_type: (Recipient::Device, Type::Standard, Direction::In).into(),
            request: RequestCode::GetDescriptor,
            value: (desc_type as u16) << 8 | desc_index as u16,
            index: 0,
            length: buf.len().try_into().unwrap(),
        };
        self.control_transfer(id, setup, Some(buf))
    }
}
