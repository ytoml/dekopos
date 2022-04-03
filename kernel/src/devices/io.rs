use core::arch::global_asm;

global_asm!(
    ".global io_write32",
    "io_write32:",
    "mov dx, di",
    "mov eax, esi",
    "out dx, eax",
    "ret",
);

global_asm!(
    ".global io_read32",
    "io_read32:",
    "mov dx, di",
    "in eax, dx",
    "ret",
);

extern "sysv64" {
    fn io_write32(addr: u16, value: u32);
    fn io_read32(addr: u16) -> u32;
}

/// This implementation assumes x86_64.
pub trait IoAccess {
    fn addr(&self) -> IoAddr;

    unsafe fn write(&self, value: u32) {
        io_write32(self.addr().0, value)
    }

    unsafe fn read(&self) -> u32 {
        io_read32(self.addr().0)
    }
}

#[derive(Debug, Default, PartialEq, Eq, PartialOrd, Ord, Clone, Copy)]
pub struct IoAddr(u16);

#[derive(Debug, Default)]
pub struct IoPort {
    addr: IoAddr,
}

impl IoPort {
    pub const PCI_CONFIG_ADDR: Self = Self::new(0x0cf8);
    pub const PCI_CONFIG_DATA: Self = Self::new(0x0cfc);

    pub const fn new(addr: u16) -> Self {
        Self { addr: IoAddr(addr) }
    }
}

impl IoAccess for IoPort {
    fn addr(&self) -> IoAddr {
        self.addr
    }
}
