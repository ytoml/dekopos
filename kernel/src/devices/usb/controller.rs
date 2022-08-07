use bit_field::BitField;
use xhci::registers::doorbell::Register as DoorbellRegister;
use xhci::Registers;

use xhci::extended_capabilities::List;
// use xhci::registers::operational::UsbCommandRegister;
use xhci::ring::trb::event::{CommandCompletion, CompletionCode, PortStatusChange, TransferEvent};

use crate::devices::interrupts;
use crate::devices::pci::PciConfig;
use crate::utils::{VolatileCell, VolatileReadAt, VolatileWriteAt};

use super::driver::DeviceManager;
use super::error::{Error, Result};
use super::mem::{ReadWrite, ReadWriteArray, UsbAllocator, UsbMapper, Vec};
use super::status::{HcOsOwned, HcStatus, Resetted, Running};
use super::xhci::context;
use super::xhci::ring::{self, CommandRing, EventRing, TrbC, TrbE};
use super::{
    Capability, Doorbell, InterruptRegisters, Operational, PortRegisters, Runtime, CR_SIZE,
    ER_SIZE, MAX_SLOTS, N_INTR, N_PORTS,
};

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

#[derive(Debug)]
pub struct HostController<S> {
    status: HcOsOwned<S>,
    inner: Controller,
}

#[derive(Debug)]
/// xHC Controller.
pub struct Controller {
    mmio_base: usize,

    cap: Capability,
    op: Operational,
    rt: Runtime,

    intr_regs: InterruptRegisters,
    n_intr: usize,

    cr: CommandRing,
    cr_doorbell: Doorbell,
    er: EventRing, // TODO: Multiple Event Ring management with ERST.

    device_manager: DeviceManager,

    // Port status must be atomically configured
    // because enabling slot includes issuing Command TRB(EnableSlot)
    // and receiving Event TRB(CommandCompletion).
    // Need way to know which port should be mapped to the slot that CommandCompletion specifies.
    // NOTE: when addressing_port == 0, there are no ports on configuration.
    port_regs: PortRegisters,
    port_phases: Vec<VolatileCell<Phase>>,
    addressing_port: VolatileCell<usize>,

    max_ports: usize,
    port_to_slot: Vec<Option<u8>>, // Need to be volatile?
}

impl HostController<Resetted> {
    pub fn init(&mut self, pci_config: PciConfig) {
        // TODO: self.request_hc_ownership();
        self.inner.init(pci_config)
    }

    /// Start xHC. Note that some settings will be prohibited or ignored after calling this.
    /// For example, MaxSlotsEn must not be changed, and CRCR's Command Ring Pointer will be immutable.
    pub fn run(mut self) -> HostController<Running> {
        HostController {
            status: self.status.start(&mut self.inner.op),
            inner: self.inner,
        }
    }
}

impl HostController<Running> {
    pub fn has_unprocessed_events(&self) -> bool {
        self.inner.has_unprocessed_events()
    }

    pub fn process_events(&mut self) -> Result<()> {
        self.inner.process_events()
    }
}

impl HostController<Resetted> {
    /// # Safety
    /// Caller must create this only once and access xHCI related registers only through this struct.
    pub unsafe fn new(mmio_base: usize) -> Self {
        Self::new_inner(mmio_base)
    }

    fn new_inner(mmio_base: usize) -> Self {
        let Registers {
            capability: mut cap,
            operational: mut op,
            port_register_set: port_regs,
            runtime: rt,
            interrupt_register_set: mut intr_regs,
            .. // ignoring doorbell (manually construct later again)
        } = unsafe { Registers::new(mmio_base, UsbMapper) };

        let max_ports = usize::from(cap.hcsparams1.read_volatile().number_of_ports()).min(N_PORTS);
        let port_phases =
            vec_no_realloc![VolatileCell::new(Phase::NotConnected); max_ports; UsbAllocator];
        let port_to_slot = vec_no_realloc![None; max_ports; UsbAllocator];

        let status =
            unsafe { HcStatus::new().request_hc_ownership(mmio_base, &cap) }.reset(&mut op);

        let er = unsafe { EventRing::new_primary(ER_SIZE, &mut intr_regs, &status) };
        let cr = unsafe { CommandRing::new(CR_SIZE, &mut op, &status) };
        let (device_manager, cr_doorbell) = create_and_register_dcbaa(mmio_base, &mut cap, &mut op);

        log::info!(
            "
HostController - Allocation:
    command ring:   {:#x},
    event ring:     {:#x},
    er seg table:   {:#x},
    dev ctx array:  {:#x},
",
            cr.head_addr(),
            er.head_addr(),
            er.seg_table_head_addr(),
            device_manager.dcbaa_pointer(),
        );

        HostController {
            status,
            inner: Controller {
                mmio_base,
                cap,
                op,
                rt,
                intr_regs,
                cr,
                cr_doorbell,
                er,
                device_manager,
                n_intr: N_INTR,
                port_regs,
                port_phases,
                addressing_port: VolatileCell::new(0),
                port_to_slot,
                max_ports,
            },
        }
    }
}

fn create_and_register_dcbaa(
    mmio_base: usize,
    cap: &mut Capability,
    op: &mut Operational,
) -> (DeviceManager, ReadWrite<DoorbellRegister>) {
    let mut max_slots_enable = cap
        .hcsparams1
        .read_volatile()
        .number_of_device_slots()
        .min(MAX_SLOTS);

    // Just set MaxSlots to MaxSlotsEnabled (no restriction imposed).
    op.config.update_volatile(|r| {
        r.set_max_device_slots_enabled(max_slots_enable);
    });

    // According to driver implementation in MikanOS, DCBAA size should be
    // MaxSlotEnabled + 1 when MaxScratchPadBuffers == 0, but I'm not sure
    // because no detailed information found.
    let n_spb = {
        // It's difficult and unnecessarily redundant to handle dynamic size device context, then assumes 32 byte here.
        assert!(
            cap.hccparams1.read_volatile().context_size() == context::CSZ,
            "This driver assumes device context in {} bytes, {} bytes found.",
            context::CONTEXT_SIZE,
            if context::CONTEXT_SIZE == 32 { 64 } else { 32 },
        );
        cap.hcsparams2.read_volatile().max_scratchpad_buffers()
    };
    if (1..(max_slots_enable as u32)).contains(&n_spb) {
        max_slots_enable = n_spb.try_into().unwrap();
    }

    log::info!("device slots: {:?}", max_slots_enable);
    let doorbell_base = mmio_base + usize::try_from(cap.dboff.read_volatile().get()).unwrap();
    let (cr_doorbell, tr_doorbells) = unsafe {
        split_doorbell_regs_into_host_and_devices_contexts(doorbell_base, max_slots_enable.into())
    };

    let n = op.pagesize.read_volatile().get();
    let page_boundary = 1 << (n + 12);
    let manager = DeviceManager::new(tr_doorbells, page_boundary);
    op.dcbaap.update_volatile(|f| {
        f.set(manager.dcbaa_pointer());
    });
    (manager, cr_doorbell)
}

/// Return doorbell for Command Ring and Transfer Rings
/// Returned doorbell array (for Transfer Rings) is length
/// doorbell_base must be calculated beforehand
/// # Safety
/// [`slots_enable`] must be equal or less than MaxSlots (must be checked beforehand).
unsafe fn split_doorbell_regs_into_host_and_devices_contexts(
    doorbell_base: usize,
    slots_enable: usize, // including doorbell 0
) -> (
    ReadWrite<DoorbellRegister>,
    ReadWriteArray<DoorbellRegister>,
) {
    let others_base = doorbell_base + 4;
    let for_command_ring = ReadWrite::new(doorbell_base, UsbMapper);
    let for_transfer_rings = ReadWriteArray::new(others_base, slots_enable - 1, UsbMapper);
    (for_command_ring, for_transfer_rings)
}

impl Controller {
    fn init(&mut self, pci_config: PciConfig) {
        self.setup_interrupters(pci_config);
    }
    fn setup_interrupters(&mut self, pci_config: PciConfig) {
        for i in 0..self.n_intr {
            self.intr_regs.update_volatile_at(i, |prim| {
                prim.imod.set_interrupt_moderation_interval(4000); // 1ms
                prim.iman.clear_interrupt_pending().set_interrupt_enable();
            });
        }
        self.op.usbcmd.update_volatile(|r| {
            r.set_interrupter_enable();
        });

        use crate::devices::pci::msi::{Capability, DeriveryMode /*MsiXCapability*/};
        match pci_config.msi_capabilities().capability() {
            Capability::MsiX(c) => {
                let mut table = unsafe { c.table() };
                let lapic_id = interrupts::get_local_apic_id();
                table.update_volatile_at(0, |entry| {
                    entry
                        .message_address
                        .set_destination_id(lapic_id.try_into().unwrap());
                    entry
                        .message_data
                        .set_trigger_mode()
                        .set_level()
                        .set_derivery_mode(DeriveryMode::Fixed)
                        // Ugly hack: not good to share constants.
                        // Better with more flexible design
                        .set_vector(
                            crate::devices::interrupts::XHCI_INTVEC_ID
                                .try_into()
                                .unwrap(),
                        );
                });
            }
            c => panic!("Expected MSI-X but found: {c:?}"),
        }

        // With PCI support, MSI(-X) capabilities are not always implemented as extended capabilities.
        // Current configuration for QEMU doesn't seemt to provide one, thus MSI capabilities need to be set through PCI configuration space.
        // For detail, see 7.5 of xHCI document.
        // let hccparams1 = self.capability.hccparams1.read_volatile();
        // let mut cap = unsafe { List::new(self.mmio_base, hccparams1, UsbMapper) }.unwrap();
        // for cap in &mut cap {
        //     if let Ok(ext) = cap {
        //         log::info!("{ext:#?}");
        //     }
        // }
    }
}

impl Controller {
    fn has_unprocessed_events(&self) -> bool {
        let dequeue_pointer = self
            .intr_regs
            // TODO: Multiple event rings
            .read_volatile_at(0)
            .erdp
            .event_ring_dequeue_pointer();
        self.er.is_unprocessed(dequeue_pointer)
    }

    fn process_events(&mut self) -> Result<()> {
        let start_addr = self
            .intr_regs
            .read_volatile_at(0)
            .erdp
            .event_ring_dequeue_pointer();
        let mut events = self.er.consume(start_addr);
        for event in &mut events {
            self.device_manager.on_event(event)?;
        }
        // NOTE: ERDP[0..2] are ERDT segment index and must be written back without any changes.
        // And ERDP[3] is event handler busy bit with `rw1c`, thus write this back without any changes too.
        let seg_ix_and_handler_busy = start_addr.get_bits(0..=3);
        let dequeue_pointer = events.dequeue_pointer() | seg_ix_and_handler_busy;
        self.intr_regs.update_volatile_at(0, |prim| {
            if prim.erdp.event_handler_busy() {
                prim.erdp.clear_event_handler_busy();
            }
            prim.erdp.set_event_ring_dequeue_pointer(dequeue_pointer);
        });
        Ok(())
    }

    pub fn port_to_slot(&self, port_id: usize) -> Option<usize> {
        self.port_to_slot[port_id].map(|u| u as usize)
    }

    fn set_port_to_slot(&mut self, port_id: usize, slot_id: u8) -> Result<()> {
        if self.port_to_slot(port_id).is_some() {
            Err(Error::SlotAlreadyUsed)
        } else {
            let _ = self.port_to_slot[port_id].insert(slot_id);
            Ok(())
        }
    }

    pub fn port_enabled_at(&self, port_id: usize) -> bool {
        self.port_regs
            .read_volatile_at(port_id)
            .portsc
            .current_connect_status()
    }

    pub fn port_connected(&self, port_id: usize) -> bool {
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
                    "request_hc_ownership: Extended capability id {id} is currently not supported."
                )
                    }
                }
            }
        }
        speed.unwrap_or_else(|| {
            log::debug!("HostController::get_port_speed: No specification of Speed ID Protocol found, then fall back to one with portsc.");
            self
                .port_regs
                .read_volatile_at(port_id.into())
                .portsc
                .port_speed()
        })
    }

    pub fn reset_port(&mut self, port_id: usize) -> Result<()> {
        if !self.port_connected(port_id) {
            return Ok(());
        }

        if self.addressing_port.read_volatile() == 0 {
            Ok(())
        } else {
            match self.port_phases.read_volatile_at(port_id) {
                Phase::NotConnected | Phase::WaitingAddressed => {
                    self.addressing_port.write_volatile(port_id);
                    self.port_phases
                        .write_volatile_at(port_id, Phase::Resetting);
                    self.init_port(port_id);
                    Ok(())
                }
                _ => Err(Error::InvalidPortPhase),
            }
        }
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

    pub fn enable_slot(&mut self, port_id: usize) {
        let mut must_notify = false;
        self.port_regs.update_volatile_at(port_id, |r| {
            if r.portsc.port_enabled_disabled() && r.portsc.port_reset_change() {
                must_notify = true;
                r.portsc.clear_port_reset_change();
                self.port_phases
                    .write_volatile_at(port_id, Phase::EnablingSlot);
                use xhci::ring::trb::command::EnableSlot;

                // according to xHCI spec 7.2.2.1.4, slot_type is 0 for USB2.0/3.0, thus skip here.
                let mut enable = EnableSlot::new();
                if self.cr.producer_cycle_state() {
                    enable.set_cycle_bit();
                }
                let enabling_trb = TrbC::EnableSlot(enable);
                let _ = self.cr.push(enabling_trb);
            }
        });
        if must_notify {
            // NOTE: To notify pushing Command into ring, modify 0
            // TODO: Confirm whether this settings of stream id and target are appropriate
            self.ring_bell(0, 0);
        }
    }

    /// Notify xHC that this software issued Trb.
    /// - reg_id == 0: for command ring
    /// - reg_id @ 1..=255: for transfer ring
    fn ring_bell(&mut self, stream_id: u16, target: u8) {
        // TODO: better to implement this functionality on `Ring::push` with #[must_use].
        self.cr_doorbell.update_volatile(|r| {
            r.set_doorbell_stream_id(stream_id)
                .set_doorbell_target(target);
        });
    }
}

impl Controller {
    fn on_event(&mut self, event: TrbE) -> Result<()> {
        match event {
            TrbE::CommandCompletion(cc) => self.command_completion(cc)?,
            TrbE::PortStatusChange(sc) => self.port_status_change(sc)?,
            TrbE::TransferEvent(te) => self.transfer_event(te)?,
            trb => {
                // TODO: Add handler on other events.
                log::debug!("Hostcontroller::on_event : {trb:?}");
            }
        }
        Ok(())
    }

    fn command_completion(&mut self, cc: CommandCompletion) -> Result<()> {
        match cc.completion_code() {
            // TODO: Appropriate handling of trb completion codes
            Ok(CompletionCode::Success) => {}
            Ok(code) => return Err(Error::UnexpectedCompletionCode(code)),
            Err(code) => return Err(Error::InvalidCompletionCode(code)),
        }

        let port_id = self.addressing_port.read_volatile();
        let slot_id = cc.slot_id();
        let phase = self.port_phases.read_volatile_at(port_id);
        match (unsafe { ring::read_trb(cc.command_trb_pointer()) }, phase) {
            (Err(bytes), _) => Err(Error::UnexpectedTrbContent(bytes)),
            (Ok(TrbC::EnableSlot(_)), Phase::EnablingSlot) => {
                self.assign_slot_and_addr_to_device(port_id, slot_id)
            }
            (Ok(TrbC::AddressDevice(_)), Phase::AddressingDevice) => {
                self.initialize_device(port_id, slot_id)
            }
            (Ok(TrbC::ConfigureEndpoint(_)), Phase::ConfiguringEndpoints) => {
                todo!();
                self.complete_configuration()
            }
            _ => Err(Error::InvalidPortPhase),
        }
    }

    /// `Address Device` phase.
    fn assign_slot_and_addr_to_device(&mut self, port_id: usize, slot_id: u8) -> Result<()> {
        self.set_port_to_slot(port_id, slot_id)?;
        self.port_phases
            .write_volatile_at(port_id, Phase::AddressingDevice);
        let port_id = port_id.try_into().unwrap();
        let port_speed = self.get_port_speed(port_id);

        let cmd = self
            .device_manager
            .enable_at(slot_id, port_id, port_speed)?;
        self.cr.push(TrbC::AddressDevice(cmd));
        self.ring_bell(0, 0);

        Ok(())
    }

    fn initialize_device(&mut self, port_id: usize, slot_id: u8) -> Result<()> {
        self.port_phases
            .write_volatile_at(port_id, Phase::InitializingDevice);
        let expected_port = self.device_manager.get_port_of_device_at(slot_id)?;
        if port_id != expected_port.into() {
            return Err(Error::InvalidPortSlotMapping {
                slot_id,
                expected_port,
                found_port: port_id.try_into().unwrap(),
            });
        }
        self.device_manager.initialize_at(slot_id)
    }

    fn complete_configuration(&mut self) -> Result<()> {
        todo!()
    }

    fn port_status_change(&mut self, sc: PortStatusChange) -> Result<()> {
        let port_id = sc.port_id().into();
        match self.port_phases.read_volatile_at(port_id) {
            Phase::NotConnected => self.reset_port(port_id),
            Phase::Resetting => {
                self.enable_slot(port_id);
                Ok(())
            }
            _ => Err(Error::InvalidPortPhase),
        }
    }

    fn transfer_event(&mut self, _te: TransferEvent) -> Result<()> {
        // let port_id = self.slot_to_device(te.slot_id());
        todo!();
    }
}
