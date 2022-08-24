use crate::devices::usb::{Error, Result};
use paste;

auto_repr_tryfrom! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub enum DescriptorType: u8 {
        Device = 1,
        Configuration = 2,
        String = 3,
        Interface = 4,
        Endpoint = 5,
        Reserved6 = 6,
        Reserved7 = 7,
        InterfacePower = 8,
        Otg = 9,
        Debug = 10,
        InterfaceAssociation = 11,
        Bos = 15,
        DeviceCapability = 13,
        Hid = 33,
        SuperspeedUsbEndpointCompanion = 48,
        SuperspeedPlusIshochronousEndpointCompanion = 49,
    }
}

// TODO: getter with flexible type
macro_rules! descriptor {
    (
        $v:vis $desc_type:ident $(-$suffix:ident)? <$bytes:literal>
        $(
            ,
            $($offset:literal = $name1:ident $(# $doc1:literal)?),* $(,)?
        )?
        $(
            // Note that only specifing lo and it will be read from continuous two bytes (i.e. [lo+1, lo]).
            [double]
            $($lo:literal = $name2:ident $(# $doc2:literal)?),* $(,)?
        )?
    ) => {
        paste::paste!{
            #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
            $v struct [<$desc_type Descriptor $($suffix)?>]([u8; $bytes]);

            impl<'a> TryFrom<&'a [u8]> for [<$desc_type Descriptor $($suffix)?>] {
                type Error = &'a [u8];
                fn try_from(value: &'a [u8]) -> core::result::Result<Self, Self::Error> {
                    if value.len() < $bytes
                        || value[0] != $bytes
                        || !matches!(
                            DescriptorType::try_from(value[1]),
                            Ok(DescriptorType::$desc_type)
                        )
                    {
                        return Err(value);
                    }
                    let mut raw = [0u8; $bytes];
                    for (dst, &src) in raw.iter_mut().zip(value.iter()) {
                        *dst = src;
                    }
                    Ok(Self(raw))
                }
            }
            impl [<$desc_type Descriptor $($suffix)?>] {
                pub const SIZE: usize = $bytes;
                #[allow(unused)]
                $v const fn length(&self) -> u8 {
                    $bytes
                }

                $(
                    $(
                        $(#[doc = $doc1])?
                        #[allow(unused)]
                        $v const fn $name1(&self) -> u8 {
                            self.0[$offset]
                        }
                    )*
                )?

                $(
                    $(
                        $(#[doc = $doc2])?
                        #[allow(unused)]
                        $v const fn $name2(&self) -> u16 {
                            (self.0[$lo+1] as u16) << 8 | self.0[$lo] as u16
                        }
                    )*
                )?
            }
        }
    };
}

descriptor!(
    pub Device<18>,
    4 = device_class,
    5 = device_sub_class,
    6 = device_protocol,
    14 = manifacuturer_index # "Index of string descriptor describing manufacturer.",
    15 = product_index # "Index of string descriptor describing manufacturer.",
    16 = serial_number_index # "Index of string descriptor describing the device's serial number.",
    17 = num_configurations # "Number of possible configurations",
    [double]
    2 = bcd_usb_spec # "Binary-Coded Decimal (e.g. [`210h`] for USB 2.10).",
    8 = vendor_id,
    10 = product_id,
    12 = bcd_device # "Binary-Coded Decimal for device release.",
);
impl DeviceDescriptor {
    /// Max packet size.
    /// Return value will be 2**bMaxPacketSize0
    pub fn max_packet_size0(&self) -> u16 {
        assert!(
            self.0[7] < 16,
            "Too large value {} found for max_packet_size0",
            self.0[7]
        );
        2 << self.0[7]
    }
}

descriptor!(
    pub Configuration<9>,
    4 = num_interfaces,
    5 = configuration_value # "Value to use and argument to the SetConfiguration request to select this configuration.",
    6 = configuration_index # "Index of string descriptor describing this configuration.",
    7 = bitmap_attributes, // Ignoring verification
    8 = max_power # "Maximum power consumption in mA."
    [double]
    2 = total_length # "Total length(bytes) of data written for this configuration. It includes the size of configuration descriptor itself.",
);

descriptor!(
    pub Interface<9>,
    2 = interface_number,
    3 = alternate_setting,
    4 = num_endpoints,
    5 = interface_class,
    6 = interface_sub_class,
    7 = interface_protocol,
    8 = interface_id,
);

descriptor!(
    pub Endpoint<7>,
    2 = endpoint_address,
    3 = attributes,
    6 = interval,
    [double]
    4 = max_packet_size,
);

descriptor!(
    pub Hid-Header<6>,
    4 = country_code,
    5 = num_descriptors,
);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ClassDescriptor([u8; 3]);
impl ClassDescriptor {
    pub const fn descriptor_type(&self) -> u8 {
        self.0[0]
    }
    pub const fn descriptor_length(&self) -> u16 {
        (self.0[2] as u16) << 8 | self.0[1] as u16
    }
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct HidDescriptor<'buf> {
    header: HidDescriptorHeader,
    classes: &'buf [u8],
}
impl<'buf> HidDescriptor<'buf> {
    pub const fn num_class_descriptors(&self) -> u8 {
        self.header.num_descriptors()
    }
    pub fn class_descriptor_at(&self, i: usize) -> Option<ClassDescriptor> {
        if i >= self.num_class_descriptors() as usize {
            None
        } else {
            Some(ClassDescriptor(
                self.classes[i * 3..i * 3 + 3].try_into().unwrap(),
            ))
        }
    }
    pub fn class_descriptors_iter(&self) -> impl Iterator<Item = ClassDescriptor> + 'buf {
        // length of classes is guarranteed to be multiple of 3,
        // as far as constructed with TryFrom
        self.classes
            .chunks(3)
            .map(|content| ClassDescriptor(content.try_into().unwrap()))
    }
}
impl<'a> TryFrom<&'a [u8]> for HidDescriptor<'a> {
    type Error = &'a [u8];
    fn try_from(value: &'a [u8]) -> core::result::Result<Self, Self::Error> {
        let header = HidDescriptorHeader::try_from(value)?;
        const HEADER_OFF: usize = 6;
        let n_desc = header.num_descriptors() as usize;
        if value.len() < HEADER_OFF + n_desc * 3 {
            return Err(value);
        }
        Ok(HidDescriptor {
            header,
            classes: &value[HEADER_OFF..HEADER_OFF + n_desc * 3],
        })
    }
}
impl core::fmt::Debug for HidDescriptor<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let mut debug = f.debug_struct("HidDescriptor");
        for class in self.class_descriptors_iter() {
            debug.field("class", &class);
        }
        debug.finish()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Supported<'a> {
    Interface(InterfaceDescriptor),
    Endpoint(EndpointDescriptor),
    Hid(HidDescriptor<'a>),
}
impl<'a> TryFrom<&'a [u8]> for Supported<'a> {
    type Error = (&'a [u8], Option<DescriptorType>);
    fn try_from(value: &'a [u8]) -> core::result::Result<Self, Self::Error> {
        if value.len() < 2 {
            return Err((value, None));
        }
        macro_rules! tryfrom {
            (
                $value:ident =>
                $($desc_type:ident),* $(,)?
            ) => {
                paste::paste!{
                    match DescriptorType::try_from($value[1]) {
                        Ok(ty) => match ty {
                            $(
                                DescriptorType::$desc_type => {
                                    [<$desc_type Descriptor>]::try_from($value)
                                        .and_then(|desc| Ok(Supported::$desc_type(desc)))
                                        .map_err(|raw| (raw, Some(DescriptorType::$desc_type)))
                                }
                            )*
                            ty => Err(($value, Some(ty))),
                        }
                        Err(_) => Err(($value, None)),
                    }
                }
            };
        }
        tryfrom!(
            value =>
            Interface,
            Endpoint,
            Hid,
        )
    }
}

#[derive(Debug)]
pub struct ConfigDescReader<'a> {
    buf: &'a [u8],
    cursor: usize,
}
impl<'a> ConfigDescReader<'a> {
    /// Note that this constructor try to read first configuration descriptor.
    /// Thus, caller should not consume beforehand.
    pub fn new(buf: &'a [u8], unused_tail_len: usize) -> Result<Self> {
        let written_len = buf.len() - unused_tail_len;
        let _desc: ConfigurationDescriptor = buf.try_into().map_err(|buf| {
            log::debug!("Configuration Descriptor read failed: {buf:?}");
            Error::InvalidDescriptor
        })?;
        Ok(Self {
            buf: &buf[ConfigurationDescriptor::SIZE..written_len],
            cursor: 0,
        })
    }
}
impl<'buf> Iterator for ConfigDescReader<'buf> {
    type Item = Result<Supported<'buf>>;
    fn next(&mut self) -> Option<Self::Item> {
        if self.cursor >= self.buf.len() {
            return None;
        }

        let base = self.cursor;
        self.cursor += self.buf[base] as usize;
        Some(
            Supported::try_from(&self.buf[base..]).map_err(|(_, desc_type)| {
                if let Some(desc_type) = desc_type {
                    Error::UnsupportedDescriptor(desc_type)
                } else {
                    Error::InvalidDescriptor
                }
            }),
        )
    }
}
