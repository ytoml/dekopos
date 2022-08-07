//! Usb Driver.
//! In this module, Ring and DCBAA implementation relies on [`alloc::vec::Vec`] to
//! allocate their array structures.
//! However, such implementation potentially incur reallocation to other space than xHC knows.
//! Then, Ring and DCBAA must not use [`alloc::vec::Vec::push()`], that can cause reallocation.
pub mod class;
mod controller;
mod data_types;
mod driver;
pub mod error;
mod mem;
pub(super) mod status;
mod utils;
mod xhci;

pub use controller::HostController;
pub use error::{Error, Result};

use mem::{ReadWrite, UsbMapper};

use self::mem::ReadWriteArray;

type Doorbell = ReadWrite<::xhci::registers::doorbell::Register>;
type PortRegisters = ReadWriteArray<::xhci::registers::PortRegisterSet>;
type InterruptRegisters = ReadWriteArray<::xhci::registers::InterruptRegisterSet>;
type Capability = ::xhci::registers::Capability<UsbMapper>;
type Operational = ::xhci::registers::Operational<UsbMapper>;
type Runtime = ::xhci::registers::Runtime<UsbMapper>;

const NUM_OF_ENDPOINTS: usize = 16;
const CR_SIZE: usize = 32;
const ER_SIZE: usize = 32;
const ER_SEG_TABLE_SIZE: usize = 1;
const TR_SIZE: usize = 32;
// Only primary interrupter is used now.
const N_INTR: usize = 1;
const N_PORTS: usize = 256;
const MAX_SLOTS: u8 = 8;
