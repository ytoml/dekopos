use bit_field::BitField;

use crate::devices::usb::mem::XhcMapper;

use super::{Bar, PciConfig};
type ReadWriteSingle<T> = ::xhci::accessor::single::ReadWrite<T, XhcMapper>;
type ReadWriteArray<T> = ::xhci::accessor::array::ReadWrite<T, XhcMapper>;

/// Note that the structure of MSI Capability Register may differ among computers
// TODO: rename
#[derive(Debug, Clone, Copy)]
pub struct MsiCapabilities {
    pci_config: PciConfig,
    base: u8,
    next_pointer: u8,
    capability: Capability,
}

#[derive(Debug, Clone, Copy)]
pub enum Capability {
    Msi(MsiCapability),
    MsiX(MsiXCapability),
}

#[derive(Debug, Clone, Copy)]
pub struct MsiCapability {
    message_control: MsiMessageControl,
    message_address: MessageAddress,
    message_upper_address: u32,
    message_data: MessageData,
}

#[derive(Debug, Clone, Copy)]
pub struct MsiXCapability {
    pci_config: PciConfig, // PCI config is needed to access Bar for Table/PBA.
    message_control: MsiXMessageControl,
    table_address: u32,
    pending_bit_array_address: u32,
}

// NOTE: Unfortunately, we cannot take advantage of Mapper in accessor crate,
// because read/write on IO address space, which is different from MMIO space.
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct MsiCapabilityRegister {
    _id: u8,
    _next_ponter: u8,
    message_control: MsiMessageControl,
    table_address: u32,
    pending_bit_array_address: u32,
}

#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct MsiXCapabilityRegister {
    _id: u8,
    _next_ponter: u8,
    message_control: MsiXMessageControl,
    table_address: u32,
    pending_bit_array_address: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct MsiMessageControl(u16);
impl MsiMessageControl {
    rw_bit!(0, msi_enable, "Msi enable bit.");
    get_bits!(
        1..=3,
        multiple_message_capable,
        u8,
        "Number of capable intterupt vectors."
    );
    rw_bits!(
        4..=6,
        multiple_message_enable,
        u8,
        "Number of enabled interrupt vectors."
    );
    get_bit!(
        7,
        is_64bit_address_capable,
        "Flag that tells whether 64 bit address is available. Available if 1."
    );
    get_bit!(
        8,
        per_vector_masking_capable,
        "Flag that tells whether per-vector mask is available. Available if 1."
    );
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct MsiXMessageControl(u16);
impl MsiXMessageControl {
    get_bits!(
        0..=10,
        table_size,
        u16,
        "MSI-X table size. Note that returned value is `N-1` while actual table size is `N`."
    );
    rw_bit!(14, function_mask, "Function mask bit. If 1, all the vectors are forciblly masked, regardress of each per-vector mask bit.");
    rw_bit!(15, msi_x_enable, "Msi-X enable bit.");
}

impl MsiXCapability {
    /// # Safety
    /// Caller must guarantee that alias of reference does not exist at same time.
    /// Returned [`MsiXTableEntry`]s actually write on the memory exactly where MsiXTable exists.
    /// Thus, caller must guarantee that alias doesn't exist.
    pub unsafe fn table(&self) -> ReadWriteArray<MsiXTableEntry> {
        self.table_inner()
    }

    fn table_inner(&self) -> ReadWriteArray<MsiXTableEntry> {
        let bar_id = self.table_address.get_bits(0..=2) as u8;
        let base = match self.pci_config.bar(bar_id) {
            Bar::Memory64 { addr, .. } => addr,
            Bar::Memory32 { addr, .. } => addr as u64,
        };
        let addr = (base + (self.table_address & !0x7) as u64) as usize;
        let len = (self.message_control.get_table_size() + 1) as usize;
        unsafe { ReadWriteArray::new(addr, len, XhcMapper) }
    }

    /// # Safety
    /// Returned [`PendingBitArray`] actually writes on the memory exactly where PBA exists.
    /// Thus, caller must guarantee that alias of same PBA doesn't exist.
    pub unsafe fn pending_bit_array(&self) -> PendingBitArray {
        self.bending_bit_array_inner()
    }

    fn bending_bit_array_inner(&self) -> PendingBitArray {
        let bar_id = self.pending_bit_array_address.get_bits(0..=2) as u8;
        let base = match self.pci_config.bar(bar_id) {
            Bar::Memory64 { addr, .. } => addr,
            Bar::Memory32 { addr, .. } => addr as u64,
        };
        let addr = (base + (self.pending_bit_array_address & !0x7) as u64) as usize;
        let len = (self.message_control.get_table_size() + 1) as usize;
        let arr_len = (len + 63) / 64;
        let inner = unsafe { ReadWriteArray::new(addr, arr_len, XhcMapper) };
        PendingBitArray::new(inner, len)
    }
}

impl MsiCapabilities {
    /// # Panics
    /// This function panics if capability read form PCI space had invalid id.
    pub fn new(pci_config: PciConfig, pointer: u8) -> Self {
        let reg = pci_config.read(pointer);
        let next_pointer = reg.get_bits(8..=15) as u8;
        let capability_id = reg.get_bits(0..=7) as u8;
        let reg1 = pci_config.read(pointer + 4);
        let reg2 = pci_config.read(pointer + 8);
        // Activate MSI/MSI-X.
        let capability = match capability_id {
            0x05 => {
                let mut message_control = MsiMessageControl(reg.get_bits(16..=31) as u16);
                message_control
                    .set_msi_enable()
                    .set_multiple_message_enable(1);
                let capability_register = (message_control.0 as u32) << 16
                    | (next_pointer as u32) << 8
                    | capability_id as u32;
                pci_config.write(pointer, capability_register);
                Capability::Msi(MsiCapability {
                    message_control,
                    message_address: MessageAddress(reg1),
                    message_upper_address: reg2,
                    message_data: MessageData(pci_config.read(pointer + 12)),
                })
            }
            0x11 => {
                let mut message_control = MsiXMessageControl(reg.get_bits(16..=31) as u16);
                message_control.set_msi_x_enable();
                let capability_register = (message_control.0 as u32) << 16
                    | (next_pointer as u32) << 8
                    | capability_id as u32;
                pci_config.write(pointer, capability_register);
                Capability::MsiX(MsiXCapability {
                    pci_config,
                    message_control,
                    table_address: reg1,
                    pending_bit_array_address: reg2,
                })
            }
            _ => panic!("Invalid capability ID {capability_id} for MSI found."),
        };

        Self {
            pci_config,
            base: pointer,
            next_pointer,
            capability,
        }
    }

    pub fn next(&self) -> Option<Self> {
        if self.next_pointer != 0 {
            Some(Self::new(self.pci_config, self.next_pointer))
        } else {
            None
        }
    }

    pub fn capability(&self) -> Capability {
        self.capability
    }

    // pub fn set_message_control(&'config self) {

    // }
}

// NOTE: PCI specification say that, for all accesses to MSI-X Table and MSI-X PBA fields, software must use aligned full
// DWORD or aligned full QWORD transactions; otherwise, the result is undefined.
// To keep this rule, MSI-X table must be provided as `ReadWriteArray<MsiXTableEntry>`.
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct MsiXTableEntry {
    pub message_address: MessageAddress,
    message_upper_address: u32,
    pub message_data: MessageData,
    vector_control: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct MessageAddress(u32);
impl MessageAddress {
    rw_bit!(
        2,
        destination_mode,
        "Destination mode. For detail, see 10.11.1 of Intel SDM."
    );
    rw_bit!(
        3,
        redirection_hint,
        "Redirection hint. For detail, see 10.11.1 of Intel SDM."
    );
    rw_bits!(12..=19, destination_id, u8, "Destination id of processor.");
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct MessageData(u32);
impl MessageData {
    /// Set the interrupt vector number.
    /// # Panics
    /// If argument is not in [0x10..=0xfe]. Other value ranges are prohibited.
    pub fn set_vector(&mut self, value: u8) {
        assert!(
            0x10 <= value && value <= 0xfe,
            "Only value in [0x10..=0xfe] is valid for Message Data, but {value} passed."
        );
        self.0.set_bits(0..=7, value as u32);
    }
    set_bits!(
        8..=10,
        derivery_mode,
        DeriveryMode,
        "Set the derivery mode."
    );
    rw_bit!(14, level, "Level bit. If Trigger Mode is 0(edge), this bit will be ignored. 0 for deassert, 1 for assert.");
    rw_bit!(
        15,
        trigger_mode,
        "Trigger Mode bit. 0 for edge, 1 for level."
    );
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u8)]
pub enum DeriveryMode {
    /// Deliver the signal to all the agents listed in the destination.
    /// The Trigger Mode for fixed delivery mode can be edge or level.
    Fixed = 0,
    /// Deliver the signal to the agent that is executing at the lowest priority of all agents listed in the destination field.
    /// The trigger mode can be edge or level.
    LowestPriority = 1,
    /// The delivery mode is edge only.
    /// For systems that rely on SMI semantics, the vector field is ignored but must be programmed to all zeroes for future compatibility.
    Smi = 2,
    /// Deliver the signal to all the agents listed in the destination field. The vector information is ignored.
    /// NMI is an edge triggered interrupt regardless of the Trigger Mode Setting.
    Nmi = 4,
    /// Deliver this signal to all the agents listed in the destination field. The vector information is ignored.
    /// INIT is an edge triggered interrupt regardless of the Trigger Mode Setting.
    Init = 5,
    /// Deliver the signal to the INTR signal of all agents in the destination field (as an interrupt that originated from an 8259A compatible interrupt controller).
    /// The vector is supplied by the INTA cycle issued by the activation of the ExtINT. ExtINT is an edge triggered interrupt.
    ExtInt = 7,
}
impl From<DeriveryMode> for u32 {
    fn from(value: DeriveryMode) -> Self {
        value as u8 as u32
    }
}

#[derive(Debug)]
pub struct PendingBitArray {
    inner: ReadWriteArray<u64>,
    len: usize,
}

impl PendingBitArray {
    fn new(inner: ReadWriteArray<u64>, len: usize) -> Self {
        assert!(
            inner.len() > 0,
            "Inner buffer of zero size passed as pending bit array."
        );
        let valid_max_len = inner.len() * 64;
        let valid_min_len = valid_max_len - 63;
        assert!(len <= valid_max_len, "Specified length is too long. It must be in [{valid_min_len}, {valid_max_len}], but {len} was passed.");
        assert!(len >= valid_min_len, "Specified length is too short. It must be in [{valid_min_len}, {valid_max_len}], but {len} was passed.");
        Self { inner, len }
    }
}

// NOTE: PCI specification say that, for all accesses to MSI-X Table and MSI-X PBA fields, software must use aligned full
// DWORD or aligned full QWORD transactions; otherwise, the result is undefined.
// This rule is kept via self.inner, i.e. `ReadWriteArray`.
impl PendingBitArray {
    pub fn set(&mut self, i: usize) {
        self.check_index(i);
        self.inner.update_volatile_at(i, |packed| {
            packed.set_bit(i % 64, true);
        });
    }

    pub fn get(&self, i: usize) -> bool {
        self.check_index(i);
        self.inner.read_volatile_at(i).get_bit(i % 64)
    }

    pub fn clear(&mut self, i: usize) {
        self.check_index(i);
        self.inner.update_volatile_at(i, |packed| {
            packed.set_bit(i % 64, false);
        });
    }

    fn check_index(&self, i: usize) {
        assert!(i < self.len, "index out of range");
    }
}
