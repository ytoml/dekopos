pub type Result<T> = core::result::Result<T, Error>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    NotImplemented,
    InvalidPortPhase,
    UnexpectedCompletionCode(u8),
    InvalidSlotId,
}

impl From<Error> for anyhow::Error {
    fn from(e: Error) -> Self {
        anyhow::anyhow!("usb device error: {e:?}")
    }
}
