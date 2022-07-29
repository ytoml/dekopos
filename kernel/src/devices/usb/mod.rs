//! Usb Driver.
//! In this module, Ring and DCBAA implementation relies on [`alloc::vec::Vec`] to
//! allocate their array structures.
//! However, such implementation potentially incur reallocation to other space than xHC knows.
//! Then, Ring and DCBAA must not use [`alloc::vec::Vec::push()`], that can cause reallocation.
pub mod class;
mod context;
mod controller;
mod data_types;
mod device;
mod driver;
pub mod error;
pub(super) mod mem;
mod ring;

pub use controller::HostController;
pub use error::{Error, Result};

const NUM_OF_ENDPOINTS: usize = 16;
