use xhci::{extended_capabilities::List, registers::capability::CapabilityParameters1};

use super::{mem::UsbMapper, Capability, Operational};

pub type HcResetted = HcStatus<OsOwned, Resetted>;
pub type HcRunning = HcStatus<OsOwned, Running>;
pub type HcOsOwned<S> = HcStatus<OsOwned, S>;

#[derive(Debug)]
pub struct HcStatus<O, S> {
    owned_or_not: O,
    hc_status: S,
}

trait HcOwnedStatus {}
trait ControllerStatus {}

impl<O, S> HcStatus<O, S> {
    fn update_owned_status<ONext>(self, next: ONext) -> HcStatus<ONext, S>
    where
        ONext: HcOwnedStatus,
    {
        HcStatus {
            owned_or_not: next,
            hc_status: self.hc_status,
        }
    }

    fn update_hc_status<SNext>(self, next: SNext) -> HcStatus<O, SNext>
    where
        SNext: ControllerStatus,
    {
        HcStatus {
            owned_or_not: self.owned_or_not,
            hc_status: next,
        }
    }
}

#[derive(Debug)]
pub struct OwnerUnknown;

#[derive(Debug)]
pub struct OsOwned;

#[derive(Debug)]
pub struct Uninitalized;

#[derive(Debug)]
pub struct Resetted;

#[derive(Debug)]
pub struct Running;

impl HcOwnedStatus for OwnerUnknown {}
impl HcOwnedStatus for OsOwned {}
impl ControllerStatus for Uninitalized {}
impl ControllerStatus for Resetted {}
impl ControllerStatus for Running {}

impl HcStatus<OwnerUnknown, Uninitalized> {
    pub fn new() -> Self {
        Self {
            owned_or_not: OwnerUnknown,
            hc_status: Uninitalized,
        }
    }

    /// Ensuring hc owner is OS.
    /// # Safety
    /// User must ensure that [`mmio_base`] (and content of [`capability`]) must be valid.
    pub unsafe fn request_hc_ownership(
        self,
        mmio_base: usize,
        capability: &Capability,
    ) -> HcStatus<OsOwned, Uninitalized> {
        self.request_hc_ownership_inner(mmio_base, capability.hccparams1.read_volatile())
    }

    fn request_hc_ownership_inner(
        self,
        mmio_base: usize,
        hccparams1: CapabilityParameters1,
    ) -> HcStatus<OsOwned, Uninitalized> {
        if let Some(mut capabilities) = unsafe { List::new(mmio_base, hccparams1, UsbMapper) } {
            for r in &mut capabilities {
                use ::xhci::extended_capabilities::{ExtendedCapability, NotSupportedId};
                match r {
                    Ok(capability) => match capability {
                        ExtendedCapability::UsbLegacySupport(mut legsup) => {
                            if legsup.usblegsup.read_volatile().hc_os_owned_semaphore() {
                                log::debug!("request_hc_ownership: OS already owns xHC.");
                                break;
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
        } else {
            log::debug!("Extended capabilities are not available on this machine.");
        }
        self.update_owned_status(OsOwned)
    }
}

impl HcStatus<OsOwned, Uninitalized> {
    pub fn reset(self, op: &mut Operational) -> HcStatus<OsOwned, Resetted> {
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
        self.update_hc_status(Resetted)
    }
}

impl HcStatus<OsOwned, Resetted> {
    pub fn start(self, op: &mut Operational) -> HcStatus<OsOwned, Running> {
        op.usbcmd.update_volatile(|r| {
            r.set_run_stop();
        });
        while op.usbsts.read_volatile().hc_halted() {}
        log::debug!("xHC Started!");
        self.update_hc_status(Running)
    }
}
