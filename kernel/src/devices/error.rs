#![allow(dead_code)]
// Defines error originates in device accesses.
use super::pci;

pub type Result<T> = core::result::Result<T, DeviceError>;
pub enum DeviceError {
    Pci(pci::error::Error),
}
