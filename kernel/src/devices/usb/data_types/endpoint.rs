use bit_field::BitField;
use xhci::{context::EndpointType, ring::trb::transfer::Direction};

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
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub struct EndpointId(u8);
}
impl TryFrom<u8> for EndpointId {
    type Error = u8;
    fn try_from(value: u8) -> Result<Self, Self::Error> {
        if value >= 32 {
            Err(value)
        } else {
            Ok(EndpointId(value))
        }
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EndpointConfig {
    pub id: EndpointId,
    pub ty: EndpointType,
    pub max_backet_size: i32,
    /// Control interval for 125*2^(interval-1) us
    pub interval: i32,
}
