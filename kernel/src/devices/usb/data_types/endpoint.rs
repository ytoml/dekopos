use bit_field::BitField;
use xhci::ring::trb::transfer::Direction;

use crate::devices::usb::Error;

auto_unit_from! {
    /// wIndex
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
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

auto_unit_from! {
    #[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub struct EndpointId(u8);
}
impl TryFrom<u8> for EndpointId {
    type Error = u8;
    fn try_from(value: u8) -> core::result::Result<Self, Self::Error> {
        if value >= 32 {
            Err(value)
        } else {
            Ok(EndpointId(value))
        }
    }
}

impl core::fmt::Debug for EndpointId {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("EndpointId")
            .field("ep", &self.as_index())
            .field("direction", &self.direction())
            .finish()
    }
}

impl EndpointId {
    pub fn new(ep_index: u8, direction: Direction) -> Self {
        let mut raw = 0;
        raw.set_bits(1..=4, ep_index);
        raw.set_bit(0, direction.into());
        Self(raw)
    }

    pub const fn zeroed() -> Self {
        Self(0)
    }

    pub fn direction(&self) -> Direction {
        self.0.get_bit(0).into()
    }

    pub fn as_index(&self) -> usize {
        self.0.get_bits(1..=4) as usize
    }
    pub const DEFAULT_CONTROL: Self = Self(1); // index = 0, in_direction
}

auto_repr_tryfrom! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub enum EndpointType: u8 {
        Control = 0,
        Isochronous = 1,
        Bulk = 2,
        Interrupt = 3,
    }
}
impl TryFrom<::xhci::context::EndpointType> for EndpointType {
    type Error = Error;
    fn try_from(value: ::xhci::context::EndpointType) -> core::result::Result<Self, Self::Error> {
        type T = ::xhci::context::EndpointType;
        match value {
            T::NotValid => Err(Error::EndpointIsNotValid),
            T::Control => Ok(Self::Control),
            T::BulkIn | T::BulkOut => Ok(Self::Bulk),
            T::IsochIn | T::IsochOut => Ok(Self::Bulk),
            T::InterruptIn | T::InterruptOut => Ok(Self::Interrupt),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EndpointConfig {
    pub id: EndpointId,
    pub ty: EndpointType, // EndpointType and EndpointId have duplicated attributes(direction)
    pub max_backet_size: u16,
    /// Control interval for 125*2^(interval-1) us
    pub interval: u8,
}
