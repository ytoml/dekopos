use bit_field::BitField;
use xhci::ring::trb::transfer::{Direction, SetupStage};

auto_repr_tryfrom! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub enum Type: u8 {
        Standard = 0,
        Class = 1,
        Vendor = 2,
    }
}

auto_repr_tryfrom! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub enum Recipient: u8 {
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
    pub struct RequestType(u8)
}
impl RequestType {
    fn new(recipient: Recipient, ty: Type, direction: Direction) -> Self {
        let mut raw = 0u8;
        raw.set_bits(0..=4, recipient.into());
        raw.set_bits(5..=6, ty.into());
        raw.set_bit(7, direction.into());
        Self(raw)
    }
}
impl TryFrom<u8> for RequestType {
    type Error = u8;
    fn try_from(value: u8) -> Result<Self, Self::Error> {
        let recipient = value.get_bits(0..=4).try_into().map_err(|_| value)?;
        let ty = value.get_bits(5..=6).try_into().map_err(|_| value)?;
        let direction = value.get_bit(7).into();
        Ok(Self::new(recipient, ty, direction))
    }
}
impl From<(Recipient, Type, Direction)> for RequestType {
    fn from(value: (Recipient, Type, Direction)) -> Self {
        let (recipient, ty, direction) = value;
        Self::new(recipient, ty, direction)
    }
}

auto_repr_tryfrom! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub enum RequestCode: u8 {
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SetupData {
    pub request_type: RequestType,
    pub request: RequestCode,
    /// wIndex has different usage. See 9.3.4 in USB 3.2 specification.
    pub value: u16,
    pub index: u16,
    pub length: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum InvalidForSetup {
    RequestType(u8),
    RequestCode(u8),
}
impl TryFrom<[u32; 2]> for SetupData {
    type Error = InvalidForSetup;
    fn try_from(raw: [u32; 2]) -> Result<Self, Self::Error> {
        let request_type = (raw[0].get_bits(0..=7) as u8)
            .try_into()
            .map_err(InvalidForSetup::RequestType)?;
        let request = (raw[0].get_bits(8..=15) as u8)
            .try_into()
            .map_err(InvalidForSetup::RequestCode)?;
        let value = raw[0].get_bits(16..=31) as u16;
        let index = raw[1].get_bits(16..=31) as u16;
        let length = raw[1].get_bits(16..31) as u16;
        Ok(Self {
            request_type,
            request,
            value,
            index,
            length,
        })
    }
}
impl From<SetupData> for [u32; 2] {
    fn from(setup: SetupData) -> Self {
        let mut lo = 0;
        let mut hi = 0;
        lo.set_bits(0..=7, setup.request_type.0 as u32);
        lo.set_bits(8..=15, setup.request as u32);
        lo.set_bits(16..=31, setup.value as u32);
        hi.set_bits(0..=15, setup.length as u32);
        hi.set_bits(16..=31, setup.index as u32);
        [lo, hi]
    }
}
// Not From but TryFrom because SetupStage expose chance to
impl TryFrom<SetupStage> for SetupData {
    type Error = InvalidForSetup;
    fn try_from(value: SetupStage) -> Result<Self, Self::Error> {
        let raw = value.into_raw();
        [raw[0], raw[1]].try_into()
    }
}
impl From<SetupData> for SetupStage {
    fn from(value: SetupData) -> Self {
        let mut setup = SetupStage::new();
        setup
            .set_request_type(value.request_type.into())
            .set_request(value.request.into())
            .set_value(value.value)
            .set_length(value.length)
            .set_index(value.index);
        setup
    }
}
