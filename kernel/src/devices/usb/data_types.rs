use core::convert::Infallible;
use core::ops::{Index, IndexMut};

use bit_field::BitField;
use xhci::context::EndpointType;
use xhci::ring::trb::transfer::Direction;

use super::mem::Vec;

auto_repr_tryfrom! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub(super) enum Type: u8 {
        Standard = 0,
        Class = 1,
        Vendor = 2,
    }
}

auto_repr_tryfrom! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub(super) enum Recipient: u8 {
        Device = 0,
        Interface = 1,
        Endpoint = 2,
        Other = 3,
        VendorSpecific = 31,
    }
}

auto_unit_from! {
    /// bmRequestType
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub(super) struct RequestType(u8)
}
impl From<(Recipient, Type, Direction)> for RequestType {
    fn from(value: (Recipient, Type, Direction)) -> Self {
        let (rec, ty, dir) = value;
        let mut raw = 0u8;
        raw.set_bits(0..=4, rec.into());
        raw.set_bits(5..=6, ty.into());
        raw.set_bit(7, dir.into());
        Self(raw)
    }
}

auto_repr_tryfrom! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub(super) enum RequestCode: u8 {
        GetStatus = 0,
        ClearFeature = 1,
        SetFeature = 3,
        SetAddress = 5,
        GetDescriptor = 6,
        SetDescriptor = 7,
        GetConfiguration = 8,
        SetConfiguration = 9,
        GetInterface = 10,
        SetInterface = 11,
        SynchFrame = 12,
        SetEncryption = 13,
        GetEncryption = 14,
        SetHandshake = 15,
        GetHandshake = 16,
        SetConnection = 17,
        SetSecurityData = 18,
        GetSecurityData = 19,
        SetWUsbData = 20,
        LoopbackDataWrite = 21,
        LoopbackDataRead = 22,
        SetInterfaceDS = 23,
        SetFWStatus = 26,
        GetFWStatus = 27,
        SetSel = 48,
        SetIsochDelay = 49,
    }
}

auto_unit_from! {
    /// wIndex
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
    pub struct EndpointIndex(u16)
}
impl EndpointIndex {
    const fn zeroed() -> Self {
        EndpointIndex(0)
    }
    rw_bits!(
        0..=3,
        endpoint_number,
        u8,
        "Endpoint Number. For detail, see 9.3.4 of USB3 specification."
    );
    set_bit!(
        7,
        direction,
        "Direction. For detail, see 9.3.4 of USB3 specification."
    );
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct EndpointId(u8);
impl EndpointId {
    pub fn new(ep_index: u8, is_in_direction: bool) -> Self {
        let mut raw = 0;
        raw.set_bits(1..=4, ep_index);
        raw.set_bit(0, is_in_direction);
        Self(raw)
    }

    pub const fn zeroed() -> Self {
        Self(0)
    }

    pub fn is_in_direction(&self) -> bool {
        self.0.get_bit(0)
    }

    pub fn value(&self) -> usize {
        self.0.get_bits(1..=4) as usize
    }
    pub const DEFAULT_CONTROL: Self = Self(1); // index = 0, in_direction
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct EndpointConfig {
    pub id: EndpointId,
    pub ty: EndpointType,
    pub max_backet_size: i32,
    /// Control interval for 125*2^(interval-1) us
    pub interval: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SetupData {
    pub(super) request_type: RequestType,
    pub(super) request: RequestCode,
    pub(super) value: u16,
    pub(super) index: u16,
    pub(super) length: u16,
}
