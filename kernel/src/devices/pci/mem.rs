use core::num::NonZeroUsize;

use accessor::Mapper;

// Note that this mapper is just for memory-mapped IO
// and is different from virtual address mapper for page table.
#[derive(Debug, Clone)]
pub struct PciMapper;
impl Mapper for PciMapper {
    unsafe fn map(&mut self, phys_start: usize, _bytes: usize) -> NonZeroUsize {
        NonZeroUsize::new(phys_start).expect("physical address 0 is passed to mapper.")
    }

    fn unmap(&mut self, _virt_start: usize, _bytes: usize) {}
}
