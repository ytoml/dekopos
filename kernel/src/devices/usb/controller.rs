use bit_field::BitField;
use xhci::registers::doorbell::Register as DoorbellRegister;
use xhci::Registers;

use super::driver::DeviceManager;
use super::error::Result;
use super::mem::{ReadWrite, ReadWriteArray, UsbMapper};
use super::status::{HcOsOwned, HcStatus, Resetted, Running};
use super::xhci::context;
use super::xhci::ring::{CommandRing, EventRing, TrbC};
use super::{
    Capability, Doorbell, InterrupterRegisters, Operational, PortRegisters, Runtime, CR_SIZE,
    ER_SIZE, MAX_SLOTS, N_INTR,
};
use crate::devices::interrupts;
use crate::devices::pci::PciConfig;

#[derive(Debug)]
pub struct HostController<S> {
    status: HcOsOwned<S>,
    controller: Controller,
    device_manager: DeviceManager,
    er: EventRing,
}

#[derive(Debug)]
/// xHC Controller.
pub struct Controller {
    mmio_base: usize,

    op: Operational,
    rt: Runtime,

    intr_regs: InterrupterRegisters,
    n_intr: usize,

    cr: CommandRing,
    cr_doorbell: Doorbell,
}

impl HostController<Resetted> {
    pub fn init(&mut self, pci_config: PciConfig) {
        // TODO: self.request_hc_ownership();
        self.controller.init(pci_config)
    }

    /// Start xHC. Note that some settings will be prohibited or ignored after calling this.
    /// For example, MaxSlotsEn must not be changed, and CRCR's Command Ring Pointer will be immutable.
    pub fn run(mut self) -> HostController<Running> {
        HostController {
            status: self.status.start(&mut self.controller.op),
            controller: self.controller,
            device_manager: self.device_manager,
            er: self.er,
        }
    }
}

impl HostController<Running> {
    pub fn has_unprocessed_events(&self) -> bool {
        let dequeue_pointer = self
            .controller
            .intr_regs
            // TODO: Multiple event rings
            .interrupter(0)
            .erdp
            .read_volatile()
            .event_ring_dequeue_pointer();
        self.er.is_unprocessed(dequeue_pointer)
    }

    pub fn process_events(&mut self) -> Result<()> {
        let start_addr = self
            .controller
            .intr_regs
            .interrupter(0)
            .erdp
            .read_volatile()
            .event_ring_dequeue_pointer();
        let mut events = self.er.consume(start_addr);
        for event in &mut events {
            if let Some(command_trb) = self.device_manager.on_event(event)? {
                self.controller.issue_command(command_trb, 0, 0); // assume that stream is not utilized.
            }
        }
        // NOTE: ERDP[0..2] are ERDT segment index and must be written back without any changes.
        // And ERDP[3] is event handler busy bit with `rw1c`, thus write this back without any changes too.
        let seg_ix_and_handler_busy = start_addr.get_bits(0..=3);
        let dequeue_pointer = events.dequeue_pointer() | seg_ix_and_handler_busy;
        let prim = &mut self.controller.intr_regs.interrupter_mut(0);
        prim.erdp.update_volatile(|erdp| {
            if erdp.event_handler_busy() {
                erdp.clear_event_handler_busy();
            }
            erdp.set_event_ring_dequeue_pointer(dequeue_pointer);
        });
        Ok(())
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
            capability: cap,
            operational: mut op,
            port_register_set: port_regs,
            runtime: rt,
            interrupter_register_set: mut intr_regs,
            .. // ignoring doorbell (manually construct later again)
        } = unsafe { Registers::new(mmio_base, UsbMapper) };

        let status =
            unsafe { HcStatus::new().request_hc_ownership(mmio_base, &cap) }.reset(&mut op);

        let er = unsafe { EventRing::new_primary(ER_SIZE, &mut intr_regs, &status) };
        let cr = unsafe { CommandRing::new(CR_SIZE, &mut op, &status) };
        let (device_manager, cr_doorbell) =
            create_and_register_dcbaa(mmio_base, cap, &mut op, port_regs);

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
            controller: Controller {
                mmio_base,
                op,
                rt,
                intr_regs,
                n_intr: N_INTR,
                cr,
                cr_doorbell,
            },
            device_manager,
            er,
        }
    }
}

fn create_and_register_dcbaa(
    mmio_base: usize,
    cap: Capability,
    op: &mut Operational,
    port_regs: PortRegisters,
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
    let (cr_doorbell, tr_doorbells) = unsafe {
        register_utils::split_doorbell_regs_into_host_and_devices_contexts(
            register_utils::doorbell_base(mmio_base, &cap),
            max_slots_enable.into(),
        )
    };

    let n = op.pagesize.read_volatile().get();
    let page_boundary = 1 << (n + 12);
    let manager =
        unsafe { DeviceManager::new(mmio_base, cap, port_regs, tr_doorbells, page_boundary) };
    op.dcbaap.update_volatile(|f| {
        f.set(manager.dcbaa_pointer());
    });
    (manager, cr_doorbell)
}

mod register_utils {
    use super::*;

    #[inline]
    pub fn doorbell_base(mmio_base: usize, cap: &Capability) -> usize {
        mmio_base + usize::try_from(cap.dboff.read_volatile().get()).unwrap()
    }

    #[inline]
    fn _operational_base(mmio_base: usize, cap: &Capability) -> usize {
        mmio_base + usize::try_from(cap.caplength.read_volatile().get()).unwrap()
    }

    #[inline]
    fn _port_base(mmio_base: usize, cap: &Capability) -> usize {
        _operational_base(mmio_base, cap) + 0x400
    }

    /// Return doorbell for Command Ring and Transfer Rings
    /// Returned doorbell array (for Transfer Rings) is length
    /// doorbell_base must be calculated beforehand
    /// # Safety
    /// [`slots_enable`] must be equal or less than MaxSlots (must be checked beforehand).
    pub unsafe fn split_doorbell_regs_into_host_and_devices_contexts(
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
}

impl Controller {
    fn init(&mut self, pci_config: PciConfig) {
        self.setup_interrupters(pci_config);
    }
    fn setup_interrupters(&mut self, pci_config: PciConfig) {
        for index in 0..self.n_intr {
            let intr = &mut self.intr_regs.interrupter_mut(index);
            intr.imod.update_volatile(|imod| {
                imod.set_interrupt_moderation_interval(4000); // 1ms
            });
            intr.iman.update_volatile(|iman| {
                iman.clear_interrupt_pending().set_interrupt_enable();
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
    fn issue_command(&mut self, trb: TrbC, stream_id: u16, target: u8) {
        let _ = self.cr.push(trb);
        self.ring_bell(stream_id, target);
    }

    /// Notify xHC that this software issued Trb (for command ring).
    fn ring_bell(&mut self, stream_id: u16, target: u8) {
        // TODO: better to implement this functionality on `Ring::push` with #[must_use].
        self.cr_doorbell.update_volatile(|r| {
            r.set_doorbell_stream_id(stream_id)
                .set_doorbell_target(target);
        });
    }
}
