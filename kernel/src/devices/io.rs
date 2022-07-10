use core::arch::asm;

unsafe fn io_write32(addr: u16, value: u32) {
    asm!(
        "mov dx, {addr:x}",
        "mov eax, {val:e}",
        "out dx, eax", // can only use "dx, eax" as operands
        addr = in(reg_abcd) addr,
        val = in(reg) value,
        options(nomem, nostack),
    );
}

unsafe fn io_read32(addr: u16) -> u32 {
    let ret: u32;
    asm!(
        "mov dx, {addr:x}",
        "in eax, dx",
        "mov {ret:e}, eax",
        addr = in(reg_abcd) addr,
        ret = out(reg) ret,
        options(nomem, nostack),
    );
    ret
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
