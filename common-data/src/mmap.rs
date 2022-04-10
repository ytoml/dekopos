use core::slice;

#[derive(Debug, Clone, Copy)]
pub struct MemDesc {
    pub ty: u32,
    pub phys_start: u64,
    pub phys_end: u64,
    /// Offset between virtual and physical address
    /// This assumes that offset paging, i.e. "virt := phys + offset"
    pub offset: u64,
    pub attribute: u64,
}

#[derive(Debug, Clone, Copy)]
pub struct MemMap {
    descs: *const MemDesc,
    count: usize,
}

impl MemMap {
    pub fn from_slice(descs: &[MemDesc]) -> Self {
        Self {
            descs: descs.as_ptr(),
            count: descs.len(),
        }
    }

    pub fn as_slice(&self) -> &[MemDesc] {
        unsafe { slice::from_raw_parts(self.descs, self.count) }
    }

    pub fn count(&self) -> usize {
        self.count
    }
}

#[cfg(feature = "uefi_imp")]
const EFI_PAGE_SIZE: u64 = 0x1000;

#[cfg(feature = "uefi_imp")]
impl From<::uefi::table::boot::MemoryDescriptor> for MemDesc {
    fn from(desc: ::uefi::table::boot::MemoryDescriptor) -> Self {
        Self {
            ty: desc.ty.0,
            phys_start: desc.phys_start,
            phys_end: desc.phys_start + desc.page_count * EFI_PAGE_SIZE,
            offset: desc.virt_start - desc.phys_start,
            attribute: desc.att.bits(),
        }
    }
}
