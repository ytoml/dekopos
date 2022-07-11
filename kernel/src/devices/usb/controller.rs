use bit_field::BitField;
// use xhci::accessor::Array;
// use xhci::extended_capabilities::xhci_extended_message_interrupt::XhciExtendedMessageInterrupt;
use xhci::extended_capabilities::List;
// use xhci::registers::operational::UsbCommandRegister;
use xhci::ring::trb::event::{CommandCompletion, PortStatusChange, TransferEvent};
use xhci::Registers;

use crate::devices::interrupts;
use crate::devices::pci::PciConfig;
use crate::utils::{VolatileCell, VolatileReadAt, VolatileWriteAt};

use super::context::{self, DeviceContextBaseAddressArray};
use super::error::{Error, Result};
use super::mem::{Vec, XhcMapper, XHC_ALLOC};
use super::ring::{CommandRing, EventRing, EventRingSegmentTable, TransferRing, TrbC, TrbE};

const CR_SIZE: usize = 32;
const ER_SIZE: usize = 32;
const ER_SEG_TABLE_SIZE: usize = 1;
const TR_SIZE: usize = 32;
// Only primary interrupter is used now.
const N_INTR: usize = 1;
const N_PORTS: usize = 256;

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
    regs: Registers<XhcMapper>,
    cr: CommandRing,
    er: EventRing, // TODO: Multiple Event Ring management with ERST.
    er_seg_table: EventRingSegmentTable,
    tr: TransferRing, // TODO: Per device Transfer Ring must be managed.
    n_intr: usize,
    capabilities: List<XhcMapper>,

    // Port status must be atomically configured.
    // NOTE: when addressing_port == 0, there are no ports on configuration.
    port_phases: Vec<VolatileCell<Phase>>,
    max_ports: usize,
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
        let regs = unsafe { Registers::new(mmio_base, XhcMapper) };
        let hccparams1 = regs.capability.hccparams1.read_volatile();
        let max_ports = regs
            .capability
            .hcsparams1
            .read_volatile()
            .number_of_ports()
            .into();
        let port_phases =
            vec_no_realloc![VolatileCell::new(Phase::NotConnected); max_ports; XHC_ALLOC];
        Self {
            mmio_base,
            regs,
            cr: CommandRing::new(CR_SIZE),
            er_seg_table: EventRingSegmentTable::new(&[&er]),
            er,
            tr: TransferRing::new(TR_SIZE),
            n_intr: N_INTR,
            capabilities: unsafe { List::new(mmio_base, hccparams1, XhcMapper) }
                .expect("extended capabilities not available."),
            port_phases,
            max_ports,
            addressing_port: VolatileCell::new(0),
        }
    }
}
impl HostController {
    pub fn init(&mut self, pci_config: PciConfig) {
        log::info!("CR: {:#018x}", self.cr.head_addr());
        log::info!("ER: {:#018x}", self.er.head_addr());
        log::info!("TR: {:#018x}", self.tr.head_addr());
        log::info!("Table: {:#018x}", self.er_seg_table.head_addr());

        // TODO: self.request_hc_ownership();
        self.reset();
        self.setup_device_ctx();
        self.setup_command_ring();
        self.setup_event_ring();
        self.setup_interrupters(pci_config);
    }

    fn reset(&mut self) {
        let op = &mut self.regs.operational;
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

    fn setup_device_ctx(&mut self) {
        let n_slots = {
            let cap = &mut self.regs.capability;
            cap.hcsparams1.read_volatile().number_of_device_slots()
        };
        log::info!("device slots: {:?}", n_slots);

        {
            // Just set MaxSlots to MaxSlotsEnabled (no restriction imposed).
            let op = &mut self.regs.operational;
            op.config.update_volatile(|r| {
                r.set_max_device_slots_enabled(n_slots);
            });
        }

        // According to driver implementation in MikanOS, DCBAA size should be
        // MaxSlotEnabled + 1 when MaxScratchPadBuffers == 0, but I'm not sure
        // because no detailed information found.
        let n_spb = {
            let cap = &mut self.regs.capability;
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

        log::info!("DBCAA capacity: {:?}", capacity);

        let op = &mut self.regs.operational;
        let n = op.pagesize.read_volatile().get();
        let page_boundary = 1 << (n + 12);
        let dcbaa = DeviceContextBaseAddressArray::new(capacity, page_boundary);
        let addr = dcbaa.head_addr();
        context::init_dcbaa(dcbaa);
        op.dcbaap.update_volatile(|f| {
            f.set(addr);
        });
    }

    fn setup_command_ring(&mut self) {
        self.regs.operational.crcr.update_volatile(|r| {
            r.set_command_ring_pointer(self.cr.head_addr());

            // set same cycle bit as command ring
            if self.cr.producer_cycle_state() {
                r.set_ring_cycle_state();
            } else {
                r.clear_ring_cycle_state();
            }
        });
    }

    fn setup_event_ring(&mut self) {
        for i in 0..self.n_intr {
            self.regs.interrupt_register_set.update_volatile_at(i, |r| {
                r.erstsz.set(self.er_seg_table.size());
                r.erdp.set_event_ring_dequeue_pointer(self.er.head_addr());
                r.erstba.set(self.er_seg_table.head_addr());
            });
        }
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

    pub fn port_connected(&self, port_id: usize) -> bool {
        self.regs
            .port_register_set
            .read_volatile_at(port_id)
            .portsc
            .current_connect_status()
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
        let port_regs = &mut self.regs.port_register_set;
        port_regs.update_volatile_at(port_id, |r| {
            if r.portsc.port_enabled_disabled() && r.portsc.port_reset_change() {
                r.portsc.clear_port_reset_change();
                self.port_phases
                    .write_volatile_at(port_id, Phase::EnablingSlot);
                use xhci::ring::trb::command::EnableSlot;
                let enabling_trb = TrbC::EnableSlot(EnableSlot::new());
                self.cr.push(enabling_trb);
            }
        });
        let door_regs = &mut self.regs.doorbell;
        // NOTE: To notify pushing Command into ring, modify 0
        door_regs.update_volatile_at(0, |r| {
            // TODO: Confirm whether this settings of stream id and target are appropriate
            r.set_doorbell_stream_id(0);
            r.set_doorbell_target(0);
        })
    }

    pub fn allocate_addr_to_device(&mut self, port_id: usize) {
        todo!();
        use xhci::context::{
            /*DeviceHandler,*/ Input32Byte,  /* InputControl, InputControlHandler, */
            InputHandler, /* SlotHandler, SlotState, */
        };
        let mut input = Input32Byte::new_32byte();
        input.control_mut().set_add_context_flag(0); // enable slot context
        input.control_mut().set_add_context_flag(1); // enable 1st endpoint context
        let port_regs = &mut self.regs.port_register_set;
        port_regs.update_volatile_at(port_id, |r| {
            r.portsc.port_speed();
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
}

impl HostController {
    fn command_completion(&mut self, cc: CommandCompletion) -> Result<()> {
        match cc.completion_code() {
            // TODO: Appropriate handling of trb completion codes
            Ok(_code) => {}
            Err(code) => return Err(Error::UnexpectedCompletionCode(code)),
        }

        let command_src = cc.command_trb_pointer() as *const TrbC;
        match unsafe { command_src.read() } {
            TrbC::EnableSlot(_) => {
                let port_id = self.addressing_port.read_volatile();
                if self.port_phases.read_volatile_at(port_id) != Phase::EnablingSlot {
                    Err(Error::InvalidPortPhase)
                } else {
                    self.address_device()
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
        }
    }

    fn address_device(&mut self) -> Result<()> {
        todo!()
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

struct XhcDeviceManager {
    contexts: Vec<xhci::context::Device<{ context::CONTEXT_SIZE }>>,
}
