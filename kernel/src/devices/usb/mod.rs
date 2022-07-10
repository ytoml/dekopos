//! Usb Driver.
//! In this module, Ring and DCBAA implementation relies on [`alloc::vec::Vec`] to
//! allocate their array structures.
//! However, such implementation potentially incur reallocation to other space than xHC knows.
//! Then, Ring and DCBAA must not use [`alloc::vec::Vec::push()`], that can cause reallocation.
mod class;
mod context;
mod controller;
pub mod error;
pub(super) mod mem;
mod ring;

pub use controller::HostController;
