extern crate alloc;

use core::pin::Pin;

use bit_field::BitField;
use heapless::LinearMap;
use xhci::extended_capabilities::List;
use xhci::registers::doorbell::Register as DoorbellRegister;
use xhci::ring::trb::command::AddressDevice;
use xhci::ring::trb::event::{CommandCompletion, CompletionCode, PortStatusChange, TransferEvent};
use xhci::ring::trb::transfer::{DataStage, Direction, SetupStage};

use super::class::ClassDriver;
use super::data_types::{
    ConfigDescReader, DescriptorType, DeviceDescriptor, EndpointConfig, EndpointId, Recipient,
    RequestCode, SetupData, Supported, Type,
};
use super::mem::{BoundedAlloc64, Box, ReadWriteArray, UsbAllocator, UsbMapper, Vec};
use super::xhci::context::{DeviceContextBaseAddressArray, DeviceCtx, InputCtx};
use super::xhci::device::Device;
use super::xhci::ring::{TrbC, TrbE, TrbT};
use super::{utils, Capability, PortRegisters};
use super::{Error, Result, NUM_OF_ENDPOINTS};
use crate::utils::{VolatileCell, VolatileReadAt, VolatileWriteAt};
use xhci::context::InputHandler;

const N_EVENT_WAITERS: usize = 4;

/// Enum that represents current configuration phase of port.
/// This enum is need because root hub port must be sticked to only one port from resetting until address is allocated.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    NotConnected,
    WaitingAddressed,
    Resetting,
    EnablingSlot,
    AddressingDevice,
    InitializingDevice,
    ConfiguringEndpoints,
    Configured,
}

/// Manager for device per slot.
/// This struct offers abstracted operations.
/// Note that xHCI related (detailed) oprations are implemented in [`super::xhci::device::Device`]
#[derive(Debug)]
pub(super) struct DeviceManager {
    mmio_base: usize,
    drivers: Vec<Option<Driver>>,
    dcbaa: DeviceContextBaseAddressArray,
    // doorbells for each device.
    // Note that doorbells[i] is for slotid i+1.
    doorbells: ReadWriteArray<DoorbellRegister>,

    cap: Capability,

    // Port status must be atomically configured
    // because enabling slot includes issuing Command TRB(EnableSlot)
    // and receiving Event TRB(CommandCompletion).
    // Need way to know which port should be mapped to the slot that CommandCompletion specifies.
    // NOTE: when addressing_port is [`None`], there are no ports on configuration.
    port_regs: PortRegisters,
    port_phases: Vec<VolatileCell<Phase>>,
    addressing_port: VolatileCell<Option<usize>>,

    port_to_slot: Vec<Option<u8>>, // Need to be volatile?

    // Page boundary used to appropriately allocate device contexts.
    ctx_alloc: BoundedAlloc64,
}
impl DeviceManager {
    /// # Safety
    /// Caller must ensure that registers this struct manages are not touched from other functions.
    /// Also, note that this internally touches [`::xhci::extended_capabilities::List`].
    pub unsafe fn new(
        mmio_base: usize,
        cap: Capability,
        port_regs: PortRegisters,
        doorbells: ReadWriteArray<DoorbellRegister>,
        page_boundary: u64,
    ) -> Self {
        let max_slots_enabled = doorbells.len();
        let max_ports = port_regs.len();
        assert!(max_slots_enabled > 0, "At least 1 slot must be allocated.");

        let port_phases =
            vec_no_realloc![VolatileCell::new(Phase::NotConnected); max_ports; UsbAllocator];
        let port_to_slot = vec_no_realloc![None; max_ports; UsbAllocator];

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
            mmio_base,
            drivers,
            dcbaa,
            doorbells,
            cap,
            port_regs,
            port_phases,
            port_to_slot,
            addressing_port: VolatileCell::new(None),
            ctx_alloc: BoundedAlloc64::new(page_boundary),
        }
    }

    fn set_addressing_port(&mut self, port_id: usize) {
        assert!(port_id > 0, "port_id must be positive.");
        self.addressing_port.write_volatile(port_id.into())
    }

    fn clear_addressing_port(&mut self) {
        self.addressing_port.update_volatile(|p| {
            let _ = p.take();
        })
    }

    fn try_get_addressing_port(&self) -> Result<usize> {
        self.addressing_port
            .read_volatile()
            .ok_or(Error::NoAddressingPortFoundWhileExpected)
    }

    fn set_port_to_slot(&mut self, port_id: u8, slot_id: u8) -> Result<()> {
        if let Some(found_slot) = self.port_to_slot[slot_id as usize] {
            Err(Error::PortAlreadyMappedToSlot {
                port_id,
                tried_slot: slot_id,
                found_slot,
            })
        } else if self.drivers[slot_id as usize].is_some() {
            Err(Error::DeviceAlreadyAllocatedForSlot(slot_id))
        } else {
            let _ = self.port_to_slot[port_id as usize].insert(slot_id);
            Ok(())
        }
    }

    fn port_enabled_at(&self, port_id: usize) -> bool {
        self.port_regs
            .read_volatile_at(port_id)
            .portsc
            .current_connect_status()
    }

    fn port_connected(&self, port_id: usize) -> bool {
        self.port_regs
            .read_volatile_at(port_id)
            .portsc
            .current_connect_status()
    }

    fn get_port_speed(&self, port_id: u8) -> u8 {
        let hccparams1 = self.cap.hccparams1.read_volatile();
        let mut speed = None;
        if let Some(mut capabilities) = unsafe { List::new(self.mmio_base, hccparams1, UsbMapper) }
        {
            for r in &mut capabilities {
                use xhci::extended_capabilities::{ExtendedCapability, NotSupportedId};
                match r {
                    Ok(capability) => match capability {
                        ExtendedCapability::XhciSupportedProtocol(sp) => {
                            let header = sp.header.read_volatile();
                            let from = header.compatible_port_offset();
                            let to = from + header.compatible_port_count();
                            if (from..to).contains(&(port_id)) {
                                if let Some(psis) = sp.psis.as_ref() {
                                    let _ = speed.insert(
                                        psis.read_volatile_at((port_id - from).into())
                                            .protocol_speed_id_value(),
                                    );
                                }
                                break;
                            }
                        }
                        c => log::debug!("get_speed: ignored {c:#x?}"),
                    },
                    Err(NotSupportedId(id)) => {
                        log::warn!(
                    "get_port_speed: Extended capability id {id} is currently not supported."
                )
                    }
                }
            }
        }
        speed.unwrap_or_else(|| {
            // Fall back to portsc value.
            self.port_regs
                .read_volatile_at(port_id.into())
                .portsc
                .port_speed()
        })
    }

    // Assign slot and allocate address to device.
    fn address_device(&mut self, port_id: u8, slot_id: u8) -> Result<AddressDevice> {
        self.set_port_to_slot(port_id, slot_id)?;
        self.port_phases
            .write_volatile_at(port_id.into(), Phase::AddressingDevice);
        let port_speed = self.get_port_speed(port_id);

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

        let device = Device::new_with_ep0_enabled(dev_ctx, inp_ctx);
        let driver = Driver::new(device);

        self.dcbaa.register(slot_id, dev_ctx_ptr_raw);
        self.register_driver(slot_id, driver);

        Ok(*AddressDevice::new()
            .set_input_context_pointer(inp_ctx_ptr_raw as u64)
            .set_slot_id(slot_id))
    }

    pub fn init_port(&mut self, port_id: usize) {
        // If USB3, this reset procedure is redundant.
        // However, it's simpler to always reset (and is valid for both types of USB).
        let mut must_wait = false;
        self.port_regs.update_volatile_at(port_id, |r| {
            if r.portsc.current_connect_status() && r.portsc.connect_status_change() {
                must_wait = true;
                r.portsc.set_port_reset().clear_connect_status_change();
            }
        });
        if must_wait {
            while !self.port_regs.read_volatile_at(port_id).portsc.port_reset() {}
        }
    }

    fn reset_port(&mut self, port_id: usize) -> Result<()> {
        if !self.port_connected(port_id) {
            return Ok(());
        }

        if let Some(port_id) = self.addressing_port.read_volatile() {
            match self.port_phases.read_volatile_at(port_id) {
                Phase::NotConnected | Phase::WaitingAddressed => {
                    self.set_addressing_port(port_id);
                    self.port_phases
                        .write_volatile_at(port_id, Phase::Resetting);
                    self.init_port(port_id);
                    Ok(())
                }
                _ => Err(Error::InvalidPortPhase),
            }
        } else {
            Ok(())
        }
    }

    fn enable_slot(&mut self, port_id: usize) -> Option<TrbC> {
        let mut actually_enabled = false;
        self.port_regs.update_volatile_at(port_id, |r| {
            if r.portsc.port_enabled_disabled() && r.portsc.port_reset_change() {
                actually_enabled = true;
                r.portsc.clear_port_reset_change();
                self.port_phases
                    .write_volatile_at(port_id, Phase::EnablingSlot);
            }
        });

        // according to xHCI spec 7.2.2.1.4, slot_type is 0 for USB2.0/3.0, thus skip here.
        actually_enabled.then(|| {
            use ::xhci::ring::trb::command::EnableSlot;
            TrbC::EnableSlot(EnableSlot::new())
        })
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

    fn initialize_device(&mut self, port_id: u8, slot_id: u8) -> Result<()> {
        self.validate_port_slot_mapping(port_id, slot_id)?;

        self.port_phases
            .write_volatile_at(port_id.into(), Phase::InitializingDevice);
        self.get_mut_at(slot_id)?.invoke_init()
    }

    fn validate_port_slot_mapping(&self, port_id: u8, slot_id: u8) -> Result<()> {
        let expected_port = self.get_port_of_device_at(slot_id)?;
        if port_id == expected_port {
            Ok(())
        } else {
            Err(Error::InvalidPortSlotMapping {
                slot_id,
                expected_port,
                found_port: port_id.try_into().unwrap(),
            })
        }
    }

    fn get_port_of_device_at(&self, slot_id: u8) -> Result<u8> {
        self.get_device_at(slot_id)
            .map(|d| d.get_root_hub_port_number())
    }

    pub fn dcbaa_pointer(&self) -> u64 {
        self.dcbaa.head_addr()
    }

    fn register_driver(&mut self, slot_id: u8, driver: Driver) {
        let _ = self.drivers[slot_id as usize].insert(driver);
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
}

impl DeviceManager {
    pub fn on_event(&mut self, event: TrbE) -> Result<Option<TrbC>> {
        match event {
            TrbE::CommandCompletion(cc) => self.command_completion(cc),
            TrbE::PortStatusChange(sc) => self.port_status_change(sc),
            TrbE::TransferEvent(te) => self.transfer_event(te).map(|()| None),
            trb => {
                // TODO: Add handler on other events.
                log::debug!("Hostcontroller::on_event : {trb:?}");
                Ok(None)
            }
        }
    }

    fn command_completion(&mut self, cc: CommandCompletion) -> Result<Option<TrbC>> {
        match cc.completion_code() {
            // TODO: Appropriate handling of trb completion codes
            Ok(CompletionCode::Success) => {}
            Ok(code) => return Err(Error::UnexpectedCompletionCode(code)),
            Err(code) => return Err(Error::InvalidCompletionCode(code)),
        }

        let port_id: u8 = self.try_get_addressing_port()?.try_into().unwrap();
        let slot_id = cc.slot_id();
        let phase = self.port_phases.read_volatile_at(port_id.into());
        match (
            unsafe { super::xhci::ring::read_trb(cc.command_trb_pointer()) },
            phase,
        ) {
            (Err(bytes), _) => Err(Error::UnexpectedTrbContent(bytes)),
            (Ok(TrbC::EnableSlot(_)), Phase::EnablingSlot) => {
                let cmd = self.address_device(port_id, slot_id)?;
                Ok(Some(TrbC::AddressDevice(cmd)))
            }
            (Ok(TrbC::AddressDevice(_)), Phase::AddressingDevice) => {
                // port check
                todo!();
                self.initialize_device(port_id, slot_id).map(|_| None)
            }
            (Ok(TrbC::ConfigureEndpoint(_)), Phase::ConfiguringEndpoints) => {
                todo!();
                self.complete_configuration(port_id, slot_id).map(|_| None)
            }
            _ => Err(Error::InvalidPortPhase),
        }
    }

    fn port_status_change(&mut self, sc: PortStatusChange) -> Result<Option<TrbC>> {
        let port_id = sc.port_id().into();
        match self.port_phases.read_volatile_at(port_id) {
            Phase::NotConnected => self.reset_port(port_id).map(|()| None),
            Phase::Resetting => Ok(self.enable_slot(port_id)),
            _ => Err(Error::InvalidPortPhase),
        }
    }

    fn transfer_event(&mut self, te: TransferEvent) -> Result<()> {
        let slot_id = te.slot_id();
        self.get_mut_at(slot_id)?.transfer_event(te)
    }

    fn complete_configuration(&mut self, _port_id: u8, _slot_id: u8) -> Result<()> {
        todo!();
        // self.get_port_of_device_at(slot_id)?;
        Ok(())
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
    // number of configuration descriptors expected (per endpoint)
    num_configurations: Vec<Option<u8>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum State {
    Uninvoked,
    Initializing(Next),
    Ready,
}

impl State {
    fn next(self) -> Option<Self> {
        match self {
            Self::Uninvoked => Some(Self::Initializing(Next::GetConfigDesc)),
            Self::Initializing(phase) => match phase {
                Next::GetConfigDesc => Some(Self::Initializing(Next::ReadSetConfig)),
                Next::ReadSetConfig => Some(Self::Initializing(Next::SetEndpoint)),
                Next::SetEndpoint => Some(Self::Ready),
            },
            Self::Ready => None,
        }
    }
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
            num_configurations: vec_no_realloc_none![NUM_OF_ENDPOINTS; UsbAllocator],
        }
    }

    fn go_next_state(&mut self) -> Result<()> {
        if let Some(state) = self.state.next() {
            self.state = state;
            Ok(())
        } else {
            Err(Error::InvalidDeviceInitializationState(
                "devices::usb::driver::Driver::toggle_state",
            ))
        }
    }

    pub fn invoke_init(&mut self) -> Result<()> {
        if self.state != State::Uninvoked {
            return Err(Error::InvalidDeviceInitializationState(
                "devices::usb::driver::Driver::invoke_init",
            ));
        }
        self.get_device_desc(EndpointId::DEFAULT_CONTROL)?;
        self.go_next_state()
    }

    /// Returned buffer is where device descriptor will be written
    pub fn get_device_desc(&mut self, id: EndpointId) -> Result<()> {
        // 18 bytes of array required, but just in case larger allocation
        let buf =
            Pin::new(vec_no_realloc![0u8; DeviceDescriptor::SIZE; UsbAllocator].into_boxed_slice());
        self.get_descriptor(id, DescriptorType::Device, 0, buf)
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
        match te.completion_code() {
            Ok(CompletionCode::Success) | Ok(CompletionCode::ShortPacket) => {}
            code => {
                log::debug!("Device::transfer_event: Invalid command completion code {code:?}")
            }
        }
        let issuer_ptr = te.trb_pointer();
        let unused_tail_len: usize = te.trb_transfer_length().try_into().unwrap(); // specify length
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
                            let setup: SetupData =
                                setup.try_into().map_err(Error::InvalidSetupStageTrb)?;
                            self.on_control_completed(id, setup, unused_tail_len)
                        }
                    }
                }
                _ => Err(Error::InvalidPortPhase),
            },
        }
    }

    pub fn on_control_completed(
        &mut self,
        id: EndpointId,
        _setup: SetupData,
        unused_tail_len: usize,
    ) -> Result<()> {
        // TODO: interpret descriptor
        match self.state {
            State::Ready => Ok(()),
            State::Initializing(next) => {
                match next {
                    Next::GetConfigDesc => self.get_config_desc(id)?,
                    Next::ReadSetConfig => self.read_and_set_config(id, unused_tail_len)?,
                    Next::SetEndpoint => self.set_endpoints()?,
                }
                self.go_next_state()
            }
            State::Uninvoked => Err(Error::InvalidDeviceInitializationState(
                "devices::usb::device::Device::on_control_completed",
            )),
        }
    }

    // take descriptor buffer from specified endpoint
    fn drain_desc_buf(&mut self, id: EndpointId) -> Result<Box<[u8]>> {
        self.desc_bufs[id.as_index()]
            .take()
            .map(Pin::into_inner)
            .ok_or(Error::DescriptorBufferNotAllocated)
    }

    // init phase 1
    fn get_config_desc(&mut self, id: EndpointId) -> Result<()> {
        let buf = self.drain_desc_buf(id)?;
        let desc: DeviceDescriptor = buf.as_ref().try_into().map_err(|buf| {
            log::debug!(
                "Invalid descriptor found during initialize phase 1 on endpoint {id:?}: {buf:?}"
            );
            Error::InvalidDescriptor
        })?;
        let _ = self.num_configurations[id.as_index()].insert(desc.num_configurations());

        // relatively large allocation just in case.
        let buf = Pin::new(vec_no_realloc![0u8; 128; UsbAllocator].into_boxed_slice());
        self.get_descriptor(
            EndpointId::DEFAULT_CONTROL,
            DescriptorType::Configuration,
            0,
            buf,
        )?;
        Ok(())
    }

    // init phase 2
    fn read_and_set_config(&mut self, id: EndpointId, unused_tail_len: usize) -> Result<()> {
        let buf = self.drain_desc_buf(id)?;
        let mut reader = ConfigDescReader::new(buf.as_ref(), unused_tail_len)?;
        while let Some(sup) = reader.next() {
            let if_desc = match sup? {
                Supported::Interface(desc) => desc,
                _ => return Err(Error::InvalidlyOrderedDescriptorFound),
            };
            let n_endpoints = if_desc.num_endpoints();
            let mut configs = Vec::with_capacity_in(n_endpoints as usize, UsbAllocator);
            for _ in 0..n_endpoints {
                let desc = reader.next().expect("number of descriptors conflicts.")?;
                match desc {
                    Supported::Interface(_) => return Err(Error::InvalidlyOrderedDescriptorFound),
                    Supported::Endpoint(desc) => {
                        let id = desc.endpoint_address().try_into().unwrap();
                        let config = EndpointConfig {
                            id,
                            ty: desc.attributes().get_bits(0..=1).try_into().unwrap(),
                            max_backet_size: desc.max_packet_size(),
                            interval: desc.interval(),
                        };
                        configs.push(config);
                        todo!()
                    }
                    Supported::Hid(desc) => {
                        log::debug!("{desc:?}");
                    }
                }
            }
        }
        Ok(())
    }

    // init phase 3
    fn set_endpoints(&mut self) -> Result<()> {
        self.device.set_endpoints()
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

    // TODO: make sure passed buffer has enough size.
    fn get_descriptor(
        &mut self,
        id: EndpointId,
        desc_type: DescriptorType,
        desc_index: u8,
        mut buf: Pin<Box<[u8]>>,
    ) -> Result<()> {
        let desc_type: u8 = desc_type.into();
        let setup = SetupData {
            request_type: (Recipient::Device, Type::Standard, Direction::In).into(),
            request: RequestCode::GetDescriptor,
            value: (desc_type as u16) << 8 | desc_index as u16,
            index: 0,
            length: buf.len().try_into().unwrap(),
        };
        self.control_transfer(id, setup, Some(&mut buf))?;
        let _ = self.desc_bufs[id.as_index()].insert(buf);
        Ok(())
    }
}
