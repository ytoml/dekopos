use xhci::ring::trb::event::CompletionCode;

use super::data_types::{DescriptorType, InvalidForSetup, SetupData};

pub type Result<T> = core::result::Result<T, Error>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    Unimplemented(&'static str),
    InvalidPortPhase,
    InvalidCompletionCode(u8),
    UnexpectedCompletionCode(CompletionCode),
    InvalidSlotId,
    InvalidPortSlotMapping {
        slot_id: u8,
        expected_port: u8,
        found_port: u8,
    },
    InvalidHidPhase(&'static str),
    InvalidDeviceInitializationState(&'static str),
    SlotAlreadyUsed,
    DeviceAlreadyAllocatedForSlot(u8),
    DeviceNotAllocatedForSlot(u8),
    TransferFailed,
    TransferRingNotAllocatedForDevice,
    TransferRingDuplicatedForSameDci,
    UnexpectedTrbContent([u32; 4]),
    InvalidSetupStageTrb(InvalidForSetup),
    InvalidTransferDirection,
    InvalidTransferLength(usize),
    NoCorrespondingIssuerTrb(u64),
    TrbIssuerMapFull,
    TrbAddressConflicts(u64),
    ClassDriverNotFoundWithIndex(usize),
    ClassDriverNotFoundWithSetupData(SetupData),
    ClassDriverResitrationFailed(SetupData),
    InvalidDescriptor,
    InvalidlyOrderedDescriptorFound,
    UnsupportedDescriptor(DescriptorType),
    DescriptorBufferNotAllocated,
    DescriptorLost {
        expected_addr: u64,
        found_addr: u64,
    },
    EventWaitersFull,
}

impl From<Error> for anyhow::Error {
    fn from(e: Error) -> Self {
        anyhow::anyhow!("usb device error: {e:?}")
    }
}
