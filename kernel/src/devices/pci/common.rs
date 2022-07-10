#![allow(dead_code)]
use bit_field::BitField;

use super::msi::MsiCapabilities;
use crate::devices::io::{IoAccess, IoPort};

/// PCI configuration address
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
pub struct HeaderType(u8);

impl HeaderType {
    #[inline]
    pub fn is_single_function(&self) -> bool {
        !self.0.get_bit(7)
    }

    #[inline]
    pub fn as_raw(&self) -> u8 {
        self.0
    }
}

#[derive(Debug, Default, PartialEq, Eq, PartialOrd, Ord, Clone, Copy)]
pub struct ClassCode {
    base_class: u8,
    sub_class: u8,
    interface: u8,
    revision: u8,
}

impl ClassCode {
    #[inline]
    fn from_u32(code: u32) -> Self {
        Self {
            base_class: code.get_bits(24..32) as u8,
            sub_class: code.get_bits(16..24) as u8,
            interface: code.get_bits(8..16) as u8,
            revision: code.get_bits(0..8) as u8,
        }
    }

    #[inline]
    pub fn base_class(&self) -> u8 {
        self.base_class
    }

    #[inline]
    pub fn sub_class(&self) -> u8 {
        self.sub_class
    }

    #[inline]
    pub fn interface(&self) -> u8 {
        self.interface
    }

    #[inline]
    pub fn revision(&self) -> u8 {
        self.revision
    }

    #[inline]
    pub fn is_inter_pci_bridge(&self) -> bool {
        self.base_class() == 0x06 && self.sub_class() == 0x04
    }

    #[inline]
    pub fn is_xhci(&self) -> bool {
        self.interface() == 0x30
    }

    #[inline]
    pub fn is_ehci(&self) -> bool {
        self.interface() == 0x20
    }

    #[inline]
    pub fn is_serial_controller(&self) -> bool {
        self.base_class() == 0x0c
    }

    #[inline]
    pub fn is_usb_xhci(&self) -> bool {
        self.is_serial_controller() && self.sub_class() == 0x03 && self.is_xhci()
    }

    #[inline]
    pub fn as_raw(&self) -> u32 {
        (self.base_class() as u32) << 24
            | (self.sub_class() as u32) << 16
            | (self.interface() as u32) << 8
            | self.revision() as u32
    }
}

#[derive(Debug, Default, PartialEq, Eq, PartialOrd, Ord, Clone, Copy)]
pub struct VendorId(u16);

impl VendorId {
    #[inline]
    pub fn is_valid(&self) -> bool {
        self.0 != 0xffff
    }

    #[inline]
    pub fn is_intel(&self) -> bool {
        self.0 == 0x8086
    }

    #[inline]
    pub fn as_raw(&self) -> u16 {
        self.0
    }
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Copy)]
pub enum Bar {
    Memory32 { addr: u32, prefetchable: bool },
    Memory64 { addr: u64, prefetchable: bool },
}

const MAX_BARS: u8 = 6;
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
    pub(super) fn read(&self, reg_addr: u8) -> u32 {
        unsafe { read_pci_config(PciAddr::new(self.bus, self.device, self.func, reg_addr)) }
    }

    #[inline]
    pub(super) fn write(&self, reg_addr: u8, value: u32) {
        unsafe {
            write_pci_config(
                PciAddr::new(self.bus, self.device, self.func, reg_addr),
                value,
            )
        }
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
        ClassCode::from_u32(self.read(0x08))
    }

    #[inline]
    pub fn msi_capabilities(&self) -> MsiCapabilities {
        let pointer = self.read(0x34).get_bits(0..8) as u8;
        MsiCapabilities::new(*self, pointer)
    }

    #[inline]
    /// # Panics
    /// This method panics if [`id`] >= 6 provided.
    pub fn bar(&self, id: u8) -> Bar {
        if id >= MAX_BARS {
            panic!("Id for bar must be <= 5.");
        }

        let bar = self.read(bar_addr(id));
        let addr = bar & !0x0f; // removing flags (4 bits from LSB)
        let prefetchable = bar.get_bit(3);
        match bar.get_bits(1..3) {
            0b00 => Bar::Memory32 { addr, prefetchable },
            0b10 => {
                if id == MAX_BARS - 1 {
                    // Expected to be 32 bit address (implied with location, no space for 64 bit)
                    panic!("Expected to be 32 bit address but flag specifies 64 bit. Some fault might occur in PCI configuration space.");
                } else {
                    let upper = self.read(bar_addr(id + 1));
                    let addr = (upper as u64) << 32 | addr as u64;
                    Bar::Memory64 { addr, prefetchable }
                }
            }
            _ => panic!("Invalid bar specification found. Some fault might occur in PCI configuration space."),
        }
    }

    #[inline]
    pub fn bus_numbers(&self) -> u32 {
        self.read(bar_addr(2))
    }

    #[inline]
    pub fn secondary_bus(&self) -> u8 {
        self.bus_numbers().get_bits(8..16) as u8
    }
}

#[inline]
fn bar_addr(id: u8) -> u8 {
    0x10 + 0x04 * id
}

#[derive(Debug, Clone, Copy)]
pub struct PciDevice {
    config: PciConfig,
    class_code: ClassCode,
    header_type: HeaderType,
    vendor_id: VendorId,
}

impl PciDevice {
    #[inline]
    pub fn from_config(config: PciConfig) -> PciDevice {
        // This design should be refined in the future to balance the number of access to io port.
        // So far, every callings of this constructor accesses port 3 times.
        let class_code = config.class_code();
        let header_type = config.header_type();
        let vendor_id = config.vendor_id();
        PciDevice {
            config,
            class_code,
            header_type,
            vendor_id,
        }
    }

    #[inline]
    pub fn config(&self) -> PciConfig {
        self.config
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
        self.vendor_id
    }

    #[inline]
    pub fn class_code(&self) -> ClassCode {
        self.class_code
    }

    #[inline]
    /// # Panics
    /// This method panics if [`id`] >= 6 provided.
    pub fn bar(&self, id: u8) -> Bar {
        self.config.bar(id)
    }
}
