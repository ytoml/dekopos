#![allow(dead_code)]
use bit_field::BitField;

use crate::devices::io::{IoAccess, IoPort};

/// PCI configuration adress
///
/// ```ignore
///  +--------+---------------+-----------------------------------------------------------------+
///  | bit    | meanings      | remarks                                                         |
///  +--------+---------------+-----------------------------------------------------------------+
///  | 31     | enable bit    | if 1, the data written into CONFIG_DATA will be sent to device. |
///  +--------+---------------+-----------------------------------------------------------------+
///  | 24..31 | reserved      | must be 0.                                                      |
///  +--------+---------------+-----------------------------------------------------------------+
///  | 16..24 | bus number    |                                                                 |
///  +--------+---------------+-----------------------------------------------------------------+
///  | 11..16 | device number |                                                                 |
///  +--------+---------------+-----------------------------------------------------------------+
///  | 08..11 | function      |                                                                 |
///  +--------+---------------+-----------------------------------------------------------------+
///  | 00..08 | offset        | register offset per 4bytes.                                     |
///  +--------+---------------+-----------------------------------------------------------------+
#[derive(Debug, Default, PartialEq, Eq, PartialOrd, Ord, Clone, Copy)]
pub struct PciAddr(u32);

impl PciAddr {
    /// Create new enabled address
    pub fn new(bus: u8, device: u8, func: u8, reg_addr: u8) -> Self {
        let mut addr = 0;
        addr.set_bit(31, true);
        addr.set_bits(16..24, bus as u32);
        addr.set_bits(11..16, device as u32);
        addr.set_bits(8..11, func as u32);
        addr.set_bits(0..8, reg_addr as u32);
        Self(addr)
    }

    pub fn disable(&mut self) {
        self.0.set_bit(31, false);
    }

    pub fn enable(&mut self) {
        self.0.set_bit(31, true);
    }
}

/// Access to PCI configuration space.
unsafe fn read_pci_config(addr: PciAddr) -> u32 {
    // TODO: Some atomicity should be introduced.
    // Maybe, we can remove this unsafe with atomic operation with PCI configuration registers.
    IoPort::PCI_CONFIG_ADDR.write(addr.0);
    IoPort::PCI_CONFIG_DATA.read()
}

unsafe fn write_pci_config(addr: PciAddr, value: u32) {
    IoPort::PCI_CONFIG_ADDR.write(addr.0);
    IoPort::PCI_CONFIG_DATA.write(value);
}

type DeviceId = u16;
type Status = u16;
type Command = u16;

#[derive(Debug, Default, PartialEq, Eq, PartialOrd, Ord, Clone, Copy)]
pub struct HeaderType(pub u8);

impl HeaderType {
    #[inline]
    pub fn is_single_function(&self) -> bool {
        !self.0.get_bit(7)
    }
}

#[derive(Debug, Default, PartialEq, Eq, PartialOrd, Ord, Clone, Copy)]
pub struct ClassCode(pub u32);

impl ClassCode {
    #[inline]
    pub fn is_inter_pci_bridge(&self) -> bool {
        let base = self.0.get_bits(24..32) as u8;
        let sub = self.0.get_bits(16..24) as u8;
        base == 0x06 && sub == 0x04
    }
}

#[derive(Debug, Default, PartialEq, Eq, PartialOrd, Ord, Clone, Copy)]
pub struct VendorId(pub u16);

impl VendorId {
    #[inline]
    pub fn is_valid(&self) -> bool {
        self.0 != 0xffff
    }
}

/// Struct that represents base parameters for PCI devices
#[derive(Debug, Default, Clone, Copy)]
pub struct PciConfig {
    bus: u8,
    device: u8,
    func: u8,
}

impl PciConfig {
    pub const fn new(bus: u8, device: u8, func: u8) -> Self {
        Self { bus, device, func }
    }

    #[inline]
    fn read(&self, reg_addr: u8) -> u32 {
        unsafe { read_pci_config(PciAddr::new(self.bus, self.device, self.func, reg_addr)) }
    }

    #[inline]
    pub fn vendor_id(&self) -> VendorId {
        VendorId(self.read(0x00).get_bits(0..16) as u16)
    }

    #[inline]
    pub fn header_type(&self) -> HeaderType {
        HeaderType(self.read(0x0c).get_bits(16..24) as u8)
    }

    #[inline]
    pub fn class_code(&self) -> ClassCode {
        ClassCode(self.read(0x08))
    }

    #[inline]
    pub fn bus_numbers(&self) -> u32 {
        self.read(0x18)
    }

    pub fn secondary_bus(&self) -> u8 {
        self.bus_numbers().get_bits(8..16) as u8
    }

    pub fn scan(&self) {}
}

#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct PciDevice {
    config: PciConfig,
    header_type: HeaderType,
}

impl PciDevice {
    #[inline]
    pub fn from_config(config: PciConfig) -> PciDevice {
        let header_type = config.header_type();
        PciDevice {
            config,
            header_type,
        }
    }

    #[inline]
    pub fn bus(&self) -> u8 {
        self.config.bus
    }

    #[inline]
    pub fn device_number(&self) -> u8 {
        self.config.device
    }

    #[inline]
    pub fn function(&self) -> u8 {
        self.config.func
    }

    #[inline]
    pub fn header_type(&self) -> HeaderType {
        self.header_type
    }

    #[inline]
    pub fn vendor_id(&self) -> VendorId {
        self.config.vendor_id()
    }

    #[inline]
    pub fn class_code(&self) -> ClassCode {
        self.config.class_code()
    }
}
