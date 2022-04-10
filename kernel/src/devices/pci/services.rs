use super::{PciConfig, PciDevice};
use crate::devices::pci::{self, Error};

const CAPACITY: usize = 32;
const DEVICE_MAX: u8 = 32;
const FUNC_MAX: u8 = 8;

pub struct PciDeviceService {
    devices: [Option<PciDevice>; CAPACITY],
    count: usize,
}

impl PciDeviceService {
    pub const fn new() -> Self {
        Self {
            devices: [None; CAPACITY],
            count: 0,
        }
    }

    pub fn scan_all_bus(&mut self) -> pci::Result<()> {
        let config = PciConfig::new(0, 0, 0);

        if config.header_type().is_single_function() {
            self.scan_bus(0)
        } else {
            for func in 1..FUNC_MAX {
                let config = PciConfig::new(0, 0, func);
                if config.vendor_id().is_valid() {
                    // in multi function device, function number represents which bus it accesses.
                    self.scan_bus(func)?;
                }
            }
            Ok(())
        }
    }

    fn scan_bus(&mut self, bus: u8) -> pci::Result<()> {
        for device in 0..DEVICE_MAX {
            let config = PciConfig::new(bus, device, 0);
            if config.vendor_id().is_valid() {
                self.scan_device(bus, device)?;
            }
        }
        Ok(())
    }

    fn scan_device(&mut self, bus: u8, device: u8) -> pci::Result<()> {
        let config = PciConfig::new(bus, device, 0);
        self.scan(config)?;
        if config.header_type().is_single_function() {
            return Ok(());
        }

        for func in 1..FUNC_MAX {
            let config = PciConfig::new(bus, device, func);
            if config.vendor_id().is_valid() {
                self.scan(config)?;
            }
        }
        Ok(())
    }

    fn scan(&mut self, config: PciConfig) -> pci::Result<()> {
        let pci_device = PciDevice::from_config(config);
        self.push(pci_device)?;
        if config.class_code().is_inter_pci_bridge() {
            // also scan secondary bus.
            let secondary = config.secondary_bus();
            self.scan_bus(secondary)?;
        }
        Ok(())
    }

    fn push(&mut self, pci_device: PciDevice) -> pci::Result<()> {
        if self.count >= self.devices.len() {
            return Err(Error::Full);
        }
        let _ = self.devices[self.count].insert(pci_device);
        self.count += 1;
        Ok(())
    }

    #[allow(dead_code)]
    pub fn get(&self, index: usize) -> Option<&PciDevice> {
        if index >= self.devices.len() {
            None
        } else {
            self.devices[index].as_ref()
        }
    }

    pub fn iter(&self) -> core::slice::Iter<Option<PciDevice>> {
        self.devices[0..self.count].iter()
    }

    pub fn reset(&mut self) {
        for device in self.devices[0..self.count].iter_mut() {
            let _ = device.take();
        }
        self.count = 0;
    }
}
