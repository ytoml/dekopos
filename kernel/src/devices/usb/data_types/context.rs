use super::EndpointId;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DeviceContextIndex(usize);
impl DeviceContextIndex {
    pub const EP0: Self = Self(1);

    pub fn new(raw: usize) -> Result<Self, <Self as TryFrom<usize>>::Error> {
        raw.try_into()
    }

    pub const fn into_raw(self) -> usize {
        self.0
    }
    /// Return [self.0 - 1] considering that index 0 is not for use in Device Context Base Address Array.
    pub fn as_index_from_zero(self) -> usize {
        (self.0 - 1) as usize
    }
}
impl TryFrom<usize> for DeviceContextIndex {
    type Error = usize;
    fn try_from(value: usize) -> Result<Self, Self::Error> {
        // DBCAA[0] is not for use.
        if value == 0 {
            Err(0)
        } else {
            Ok(Self(value))
        }
    }
}
impl TryFrom<EndpointId> for DeviceContextIndex {
    type Error = usize;
    fn try_from(value: EndpointId) -> Result<Self, Self::Error> {
        let raw: u8 = value.into();
        (raw as usize).try_into()
    }
}
