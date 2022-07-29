use bit_field::BitField;
// use xhci::accessor::Array;
// use xhci::extended_capabilities::xhci_extended_message_interrupt::XhciExtendedMessageInterrupt;
use xhci::extended_capabilities::List;
use xhci::ring::trb::command::AddressDevice;
// use xhci::registers::operational::UsbCommandRegister;
use xhci::ring::trb::event::{CommandCompletion, PortStatusChange, TransferEvent};

use crate::devices::interrupts;
use crate::devices::pci::PciConfig;
use crate::devices::usb::context::{EndpointCtx, SlotCtx};
use crate::utils::{VolatileCell, VolatileReadAt, VolatileUpdateAt, VolatileWriteAt};

use super::context::{self, DeviceContextBaseAddressArray, DeviceCtx, InputCtx};
use super::error::{Error, Result};
use super::mem::{Vec, XhcMapper, XHC_ALLOC};
use super::ring::{
    self, CommandRing, EventRing, EventRingSegmentTable, TransferRing, TrbC, TrbE, TrbT,
};

type Registers = xhci::Registers<XhcMapper>;

const CR_SIZE: usize = 32;
const ER_SIZE: usize = 32;
const ER_SEG_TABLE_SIZE: usize = 1;
const TR_SIZE: usize = 32;
// Only primary interrupter is used now.
const N_INTR: usize = 1;
const N_PORTS: usize = 256;
const MAX_SLOTS: u8 = 8;

/// Enum that represents current configuration phase of port.
/// This enum is need because root hub port must be sticked to only one port from resetting until address is allocated.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    NotConnected,
    WaitingAddressed,
    Resetting,
    EnablingSlot,
    AddressingDevice,
    ConfiguringEndpoints,
    Configured,
}

/// xHC Controller.
pub struct HostController {
    mmio_base: usize,
    regs: Registers,
    cr: CommandRing,
    er: EventRing, // TODO: Multiple Event Ring management with ERST.
    er_seg_table: EventRingSegmentTable,
    tr: TransferRing, // TODO: Per device Transfer Ring must be managed.
    n_intr: usize,
    // capabilities: List<XhcMapper>,
    dcbaa: DeviceContextBaseAddressArray,
    // Port status must be atomically configured.
    // NOTE: when addressing_port == 0, there are no ports on configuration.
    port_phases: Vec<VolatileCell<Phase>>,
    max_ports: usize,
    port_to_slot: Vec<Option<u8>>, // Need to be volatile?
    addressing_port: VolatileCell<usize>,
}

impl HostController {
    /// # Safety
    /// Caller must create this only once and access xHCI related registers only through this struct.
    pub unsafe fn new(mmio_base: usize) -> Self {
        Self::new_inner(mmio_base)
    }

    fn new_inner(mmio_base: usize) -> Self {
        let er = EventRing::new(ER_SIZE);
        let mut regs = unsafe { Registers::new(mmio_base, XhcMapper) };
        let max_ports: usize = regs
            .capability
            .hcsparams1
            .read_volatile()
            .number_of_ports()
            .into();
        let max_ports = max_ports.min(N_PORTS);
        let port_phases =
            vec_no_realloc![VolatileCell::new(Phase::NotConnected); max_ports; XHC_ALLOC];
        let port_to_slot = vec_no_realloc![None; max_ports; XHC_ALLOC];

        let hccparams1 = regs.capability.hccparams1.read_volatile();
        if let Some(capabilities) = unsafe { List::new(mmio_base, hccparams1, XhcMapper) } {
            request_hc_ownership(capabilities);
        } else {
            log::debug!("Extended capabilities are not available on this machine.");
        }

        reset(&mut regs);
        let dcbaa = create_and_register_dcbaa(&mut regs);
        let controller = Self {
            mmio_base,
            regs,
            cr: CommandRing::new(CR_SIZE),
            er_seg_table: EventRingSegmentTable::new(&[&er]),
            er,
            tr: TransferRing::new(TR_SIZE),
            dcbaa,
            n_intr: N_INTR,
            port_phases,
            port_to_slot,
            max_ports,
            addressing_port: VolatileCell::new(0),
        }
        .register_command_ring()
        .register_event_ring();
        log::info!(
            "
Allocation:
    command ring:   {:#x},
    event ring:     {:#x},
    transfer ring:  {:#x},
    er seg table:   {:#x},
    dev ctx array:  {:#x},
",
            controller.cr.head_addr(),
            controller.er.head_addr(),
            controller.tr.head_addr(),
            controller.er_seg_table.head_addr(),
            controller.dcbaa.head_addr(),
        );
        controller
    }

    fn register_command_ring(mut self) -> Self {
        self.regs.operational.crcr.update_volatile(|r| {
            r.set_command_ring_pointer(self.cr.head_addr());

            // set same cycle bit as command ring
            if self.cr.producer_cycle_state() {
                r.set_ring_cycle_state();
            } else {
                r.clear_ring_cycle_state();
            }
        });
        self
    }

    fn register_event_ring(mut self) -> Self {
        // TODO: Multiple Event rings
        self.regs.interrupt_register_set.update_volatile_at(0, |r| {
            r.erstsz.set(self.er_seg_table.size());
            r.erdp.set_event_ring_dequeue_pointer(self.er.head_addr());
            r.erstba.set(self.er_seg_table.head_addr());
        });
        self
    }
}

// Reference: https://github.com/uchan-nos/mikanos/blob/c1a734f594bceb0767fe630b0b2cd3fef227bf16/kernel/usb/xhci/xhci.cpp#L312
// Pitfalls explained: https://www.slideshare.net/uchan_nos/usb30-239621497?from_action=save
fn request_hc_ownership(mut capabilities: List<XhcMapper>) {
    for r in &mut capabilities {
        use xhci::extended_capabilities::usb_legacy_support_capability::UsbLegacySupport;
        use xhci::extended_capabilities::{ExtendedCapability, NotSupportedId};
        match r {
            Ok(capability) => match capability {
                ExtendedCapability::UsbLegacySupport(mut legsup) => {
                    if legsup.usblegsup.read_volatile().hc_os_owned_semaphore() {
                        log::debug!("request_hc_ownership: OS already owns xHC.");
                        return;
                    }
                    log::debug!("OS did not own xHC, thus requesting...");
                    legsup.usblegsup.update_volatile(|r| {
                        r.set_hc_os_owned_semaphore();
                    });
                    log::debug!("Wait for ownership passed...");
                    let mut reg = legsup.usblegsup.read_volatile();
                    while reg.hc_bios_owned_semaphore() || !reg.hc_os_owned_semaphore() {
                        reg = legsup.usblegsup.read_volatile();
                    }
                }
                c => log::debug!("request_hc_ownership: ignored {c:#x?}"),
            },
            Err(NotSupportedId(id)) => {
                log::warn!(
                    "request_hc_ownership: Extended capability id {id} is currently not supported."
                )
            }
        }
    }
}

fn reset(regs: &mut Registers) {
    let op = &mut regs.operational;
    // Ensure that host controller is halted before reset.
    let mut must_halt = false;
    op.usbsts.update_volatile(|r| {
        must_halt = !r.hc_halted();
    });
    if must_halt {
        op.usbcmd.update_volatile(|r| {
            r.clear_run_stop();
        });
        while !op.usbsts.read_volatile().hc_halted() {}
    }
    op.usbcmd.update_volatile(|r| {
        r.set_host_controller_reset();
    });
    while op.usbcmd.read_volatile().host_controller_reset() {}
    while op.usbsts.read_volatile().controller_not_ready() {}
}

fn create_and_register_dcbaa(regs: &mut Registers) -> DeviceContextBaseAddressArray {
    let n_slots = {
        let cap = &mut regs.capability;
        cap.hcsparams1
            .read_volatile()
            .number_of_device_slots()
            .min(MAX_SLOTS)
    };
    log::info!("device slots: {:?}", n_slots);

    {
        // Just set MaxSlots to MaxSlotsEnabled (no restriction imposed).
        let op = &mut regs.operational;
        op.config.update_volatile(|r| {
            r.set_max_device_slots_enabled(n_slots);
        });
    }

    // According to driver implementation in MikanOS, DCBAA size should be
    // MaxSlotEnabled + 1 when MaxScratchPadBuffers == 0, but I'm not sure
    // because no detailed information found.
    let n_spb = {
        let cap = &mut regs.capability;
        // It's difficult and unnecessarily redundant to handle dynamic size device context, then assumes 32 byte here.
        assert!(
            cap.hccparams1.read_volatile().context_size() == context::CSZ,
            "This driver assumes device context in {} bytes, {} bytes found.",
            context::CONTEXT_SIZE,
            if context::CONTEXT_SIZE == 32 { 64 } else { 32 },
        );
        cap.hcsparams2.read_volatile().max_scratchpad_buffers() as usize
    };
    let capacity = if n_spb > 0 {
        n_spb
    } else {
        n_slots as usize + 1
    };

    log::info!("DCBAA capacity: {:?}", capacity);

    let op = &mut regs.operational;
    let n = op.pagesize.read_volatile().get();
    let page_boundary = 1 << (n + 12);
    let dcbaa = DeviceContextBaseAddressArray::new(capacity, page_boundary);
    log::debug!("DCBAA addr: {:#x}", dcbaa.head_addr());
    op.dcbaap.update_volatile(|f| {
        f.set(dcbaa.head_addr());
    });
    dcbaa
}

impl HostController {
    pub fn init(&mut self, pci_config: PciConfig) {
        // TODO: self.request_hc_ownership();
        self.setup_interrupters(pci_config);
    }

    fn setup_interrupters(&mut self, pci_config: PciConfig) {
        for i in 0..self.n_intr {
            let intregs = &mut self.regs.interrupt_register_set;
            intregs.update_volatile_at(i, |prim| {
                prim.imod.set_interrupt_moderation_interval(4000); // 1ms
                prim.iman.clear_interrupt_pending().set_interrupt_enable();
            });
        }
        {
            let op = &mut self.regs.operational;
            op.usbcmd.update_volatile(|r| {
                r.set_interrupter_enable();
            });
        }

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
        // let hccparams1 = self.regs.capability.hccparams1.read_volatile();
        // let mut cap = unsafe { List::new(self.mmio_base, hccparams1, XhcMapper) }.unwrap();
        // for cap in &mut cap {
        //     if let Ok(ext) = cap {
        //         log::info!("{ext:#?}");
        //     }
        // }
    }
}

impl HostController {
    /// Start xHC. Note that some settings will be prohibited or ignored after calling this.
    /// For example, MaxSlotsEn must not be changed, and CRCR's Command Ring Pointer will be immutable.
    pub fn start(&mut self) {
        let op = &mut self.regs.operational;
        op.usbcmd.update_volatile(|r| {
            r.set_run_stop();
        });
        while op.usbsts.read_volatile().hc_halted() {}
        log::info!("xHC Started!");
    }

    pub fn has_unprocessed_events(&self) -> bool {
        let dequeue_pointer = self
            .regs
            .interrupt_register_set
            // TODO: Multiple event rings
            .read_volatile_at(0)
            .erdp
            .event_ring_dequeue_pointer();
        self.er.is_unprocessed(dequeue_pointer)
    }

    pub fn process_events(&mut self) {
        let start_addr = self
            .regs
            .interrupt_register_set
            .read_volatile_at(0)
            .erdp
            .event_ring_dequeue_pointer();
        let mut events = self.er.consume(start_addr);
        for event in &mut events {
            // TODO:
        }
        // NOTE: ERDP[0..2] are ERDT segment index and must be written back without any changes.
        // And ERDP[3] is event handler busy bit with `rw1c`, thus write this back without any changes too.
        let seg_ix_and_handler_busy = start_addr.get_bits(0..=3);
        let dequeue_pointer = events.dequeue_pointer() | seg_ix_and_handler_busy;
        self.regs
            .interrupt_register_set
            .update_volatile_at(0, |prim| {
                if prim.erdp.event_handler_busy() {
                    prim.erdp.clear_event_handler_busy();
                }
                prim.erdp.set_event_ring_dequeue_pointer(dequeue_pointer);
            });
    }

    pub fn port_to_slot(&self, port_id: usize) -> Option<usize> {
        self.port_to_slot[port_id].and_then(|u| Some(u as usize))
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
        self.regs
            .port_register_set
            .read_volatile_at(port_id)
            .portsc
            .current_connect_status()
    }

    pub fn port_connected(&self, port_id: usize) -> bool {
        self.regs
            .port_register_set
            .read_volatile_at(port_id)
            .portsc
            .current_connect_status()
    }

    fn get_port_speed(&self, port_id: usize) -> u8 {
        let hccparams1 = self.regs.capability.hccparams1.read_volatile();
        let mut speed = None;
        if let Some(mut capabilities) = unsafe { List::new(self.mmio_base, hccparams1, XhcMapper) }
        {
            for r in &mut capabilities {
                use xhci::extended_capabilities::usb_legacy_support_capability::UsbLegacySupport;
                use xhci::extended_capabilities::{ExtendedCapability, NotSupportedId};
                match r {
                    Ok(capability) => match capability {
                        ExtendedCapability::XhciSupportedProtocol(sp) => {
                            let header = sp.header.read_volatile();
                            let from = header.compatible_port_offset();
                            let to = from + header.compatible_port_count();
                            if (from..to).contains(&(port_id.try_into().unwrap())) {
                                if let Some(psis) = sp.psis.as_ref() {
                                    let _ = speed.insert(
                                        psis.read_volatile_at(port_id - from as usize)
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
            self.regs
                .port_register_set
                .read_volatile_at(port_id)
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
        let port_regs = &mut self.regs.port_register_set;
        port_regs.update_volatile_at(port_id, |r| {
            if r.portsc.current_connect_status() && r.portsc.connect_status_change() {
                must_wait = true;
                r.portsc.set_port_reset().clear_connect_status_change();
            }
        });
        if must_wait {
            while !port_regs.read_volatile_at(port_id).portsc.port_reset() {}
        }
    }

    pub fn enable_slot(&mut self, port_id: usize) {
        let mut must_notify = false;
        let port_regs = &mut self.regs.port_register_set;
        port_regs.update_volatile_at(port_id, |r| {
            if r.portsc.port_enabled_disabled() && r.portsc.port_reset_change() {
                must_notify = true;
                r.portsc.clear_port_reset_change();
                self.port_phases
                    .write_volatile_at(port_id, Phase::EnablingSlot);
                use xhci::ring::trb::command::EnableSlot;
                let enabling_trb = TrbC::EnableSlot(EnableSlot::new());
                self.cr.push(enabling_trb);
            }
        });
        if must_notify {
            // NOTE: To notify pushing Command into ring, modify 0
            // TODO: Confirm whether this settings of stream id and target are appropriate
            self.ring_doorbell(0, 0, 0);
        }
    }

    /// Notify xHC that this software issued Trb.
    /// - reg_id == 0: for command ring
    /// - reg_id @ 1..=255: for transfer ring
    fn ring_doorbell(&mut self, reg_id: usize, stream_id: u16, target: u8) {
        // TODO: better to implement this functionality on `Ring::push` with #[must_use].
        self.regs.doorbell.update_volatile_at(reg_id, |r| {
            r.set_doorbell_stream_id(stream_id)
                .set_doorbell_target(target);
        });
    }

    pub fn control_transfer(&mut self) {
        todo!();
        use super::ring::TrbT;
        use xhci::ring::trb::transfer::{DataStage, SetupStage, StatusStage /*TransferType*/};
        TrbT::DataStage(DataStage::new());
        TrbT::SetupStage(SetupStage::new());
        TrbT::StatusStage(StatusStage::new());
    }
}

fn get_max_packet_size(port_speed_value: u8) -> u16 {
    match port_speed_value {
        1 => unimplemented!("get_max_packet_size: Full-Speed is out of scope."),
        2 => 8,
        3 => 64,
        4 => 512,
        psiv => {
            // TODO: Add support for psiv == 5 or 6.
            log::debug!("PSIV {psiv} is not expected, thus fall back to packet size = 8");
            8
        }
    }
}

impl HostController {
    fn on_event(&mut self, trb: TrbE) -> Result<()> {
        match trb {
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
            Ok(_code) => {}
            Err(code) => return Err(Error::UnexpectedCompletionCode(code)),
        }

        match unsafe { ring::read_trb(cc.command_trb_pointer()) } {
            Err(bytes) => Err(Error::UnexpectedTrbContent(bytes)),
            Ok(trb) => match trb {
                TrbC::EnableSlot(es) => {
                    let port_id = self.addressing_port.read_volatile();
                    if self.port_phases.read_volatile_at(port_id) != Phase::EnablingSlot {
                        Err(Error::InvalidPortPhase)
                    } else {
                        self.assign_slot_and_addr_to_device(port_id, cc.slot_id())
                    }
                }
                TrbC::AddressDevice(_) => {
                    // TODO:
                    self.initialize_device()
                }
                TrbC::ConfigureEndpoint(_) => {
                    // TODO:
                    self.complete_configuration()
                }
                _ => Err(Error::InvalidPortPhase),
            },
        }
    }

    fn assign_slot_and_addr_to_device(&mut self, port_id: usize, slot_id: u8) -> Result<()> {
        use xhci::context::{
            EndpointHandler,
            /*DeviceHandler,*/ Input32Byte, /* InputControl, InputControlHandler, */
            InputHandler, SlotHandler, SlotState,
        };
        self.set_port_to_slot(port_id, slot_id)?;

        let mut input_ctx = InputCtx::new_32byte();

        // TODO: Succinct chain initialization
        // // enable slot context and 1st endpoint context
        // let mut input_ctx = init_chain! {
        //     let v = InputCtx::new_32byte();
        //     ..;
        //     v
        // };
        input_ctx.control_mut().set_add_context_flag(0);
        input_ctx.control_mut().set_add_context_flag(1);

        let mut slot_ctx = SlotCtx::new_32byte();
        slot_ctx.set_root_hub_port_number(port_id.try_into().unwrap());
        let speed = self.get_port_speed(port_id);
        slot_ctx.set_speed(speed);

        let mut ep0_ctx = EndpointCtx::new_32byte();
        ep0_ctx.set_max_burst_size(0);
        if self.tr.producer_cycle_state() {
            ep0_ctx.set_dequeue_cycle_state();
        }
        let max_packet_size = get_max_packet_size(self.get_port_speed(port_id));
        ep0_ctx.set_max_packet_size(max_packet_size);
        ep0_ctx.set_tr_dequeue_pointer(self.tr.head_addr());

        // TODO: load to dcbaa
        self.port_phases
            .write_volatile_at(port_id, Phase::AddressingDevice);

        let mut cmd = AddressDevice::new();
        cmd.set_slot_id(slot_id);
        self.cr.push(TrbC::AddressDevice(cmd));
        self.ring_doorbell(0, 0, 0);

        Ok(())
    }

    fn initialize_device(&mut self) -> Result<()> {
        todo!()
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

    fn transfer_event(&mut self, te: TransferEvent) -> Result<()> {
        // let port_id = self.slot_to_device(te.slot_id());
        todo!();
    }
}
