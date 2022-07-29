use super::{class::ClassDriver, data_types::SetupData, mem::Box};

pub type Result<T> = core::result::Result<T, Error>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    Unimplemented(&'static str),
    InvalidPortPhase,
    UnexpectedCompletionCode(u8),
    InvalidSlotId,
    InvalidHidPhase(&'static str),
    InvalidDeviceInitializationState(&'static str),
    SlotAlreadyUsed,
    TransferFailed,
    UnexpectedTrbContent([u32; 4]),
    ClassDriverNotFound(usize),
    ClassDriverResitrationFailed(SetupData),
    EventWaitersFull,
}

impl From<Error> for anyhow::Error {
    fn from(e: Error) -> Self {
        anyhow::anyhow!("usb device error: {e:?}")
    }
}
