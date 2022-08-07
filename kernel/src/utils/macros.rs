macro_rules! access_static_as_ref {
    ($p:vis $f:ident, $st:ident, $ret:ty $(, $method:ident)*) => {
        /// # Safety
        /// Caller must guarantee that there are no aliasing of mutable reference.
        #[allow(dead_code)]
        $p unsafe fn $f() -> &'static $ret {
            $st.as_ref()$(.$method())?
        }
    };
}

macro_rules! access_static_as_mut {
    ($p:vis $f:ident, $st:ident, $ret:ty $(, $method:ident)*) => {
        /// # Safety
        /// Caller must guarantee that there are no aliasing of mutable reference.
        #[allow(dead_code)]
        $p unsafe fn $f() -> &'static mut $ret {
            $st.as_mut()$(.$method())?
        }
    };
}

macro_rules! access_static_ref {
    ($p:vis $f:ident, $st:ident, $ret:ty $(, $method:ident)*) => {
        /// # Safety
        /// Caller must guarantee that there are no aliasing of mutable reference.
        #[allow(dead_code)]
        $p unsafe fn $f() -> &'static $ret {
            &$st$(.$method())?
        }
    };
}

macro_rules! access_static_mut {
    ($p:vis $f:ident, $st:ident, $ret:ty $(, $method:ident)*) => {
        /// # Safety
        /// Caller must guarantee that there are no aliasing of mutable reference.
        #[allow(dead_code)]
        $p unsafe fn $f() -> &'static mut $ret {
            &mut $st$(.$method())?
        }
    };
}

macro_rules! access_static_as_ref_unwrap {
    ($p:vis $f:ident, $st:ident, $ret:ty) => {
        access_static_as_ref!($p $f, $st, $ret, unwrap);
    };
}

macro_rules! access_static_as_mut_unwrap {
    ($p:vis $f:ident, $st:ident, $ret:ty) => {
        access_static_as_mut!($p $f, $st, $ret, unwrap);
    };
}

macro_rules! access_static_as_both_unwrap {
    ($p:vis $f:ident, $st:ident, $ret:ty) => {
        access_static_as_ref_unwrap!($p $f, $st, $ret);
        paste::paste!{
            access_static_as_mut_unwrap!($p [<$f _mut>], $st, $ret);
        }
    };
}

macro_rules! access_static_both {
    ($p:vis $f:ident, $st:ident, $ret:ty$(, $method:ident)*) => {
        access_static_ref!($p $f, $st, $ret $(, $method)*);
        paste::paste!{
            access_static_mut!($p [<$f _mut>], $st, $ret $(, $method)*);
        }
    };
}

macro_rules! access_static_as_both {
    ($p:vis $f:ident, $st:ident, $ret:ty $(, $method:ident)*) => {
        access_static_as_ref!($p $f, $st, $ret $(, $method)*);
        paste::paste!{
            access_static_as_mut!($p [<$f _mut>], $st, $ret $(, $method)*);
        }
    };
}

// This macro follows the fashion in xHCI crate.
// https://github.com/rust-osdev/xhci/blob/06d7b7a23683272ba590422c8eb4b502ad5f16cd/src/macros.rs
macro_rules! set_bits {
    ($range:expr, $method:ident, $ty:ty) => {
        paste::paste! {
            #[allow(unused)]
            pub fn [<set_ $method>](&mut self, value:$ty) -> &mut Self {
                use bit_field::BitField;
                use core::convert::TryInto;
                self.0.set_bits($range,value.try_into().unwrap());
                self
            }
        }
    };
    ($range:expr, $method:ident, $ty:ty, $doc:literal) => {
        paste::paste! {
            #[doc = $doc]
            #[allow(unused)]
            pub fn [<set_ $method>](&mut self, value:$ty) -> &mut Self {
                use bit_field::BitField;
                use core::convert::TryInto;
                self.0.set_bits($range,value.try_into().unwrap());
                self
            }
        }
    };
}

macro_rules! get_bits {
    ($range:expr, $method:ident, $ty:ty) => {
        paste::paste! {
            #[allow(unused)]
            pub fn [<get_ $method>](&self) -> $ty {
                use bit_field::BitField;
                self.0.get_bits($range).try_into().unwrap()
            }
        }
    };
    ($range:expr, $method:ident, $ty:ty, $doc:literal) => {
        paste::paste! {
            #[doc = $doc]
            #[allow(unused)]
            pub fn [<get_ $method>](&self) -> $ty {
                use bit_field::BitField;
                self.0.get_bits($range).try_into().unwrap()
            }
        }
    };
}

macro_rules! set_bit {
    ($bit:literal, $method:ident) => {
        paste::paste! {
            #[allow(unused)]
            pub fn [<set_ $method>](&mut self) -> &mut Self {
                use bit_field::BitField;
                self.0.set_bit($bit, true);
                self
            }
        }
    };

    ($bit:literal, $method:ident, $doc:literal) => {
        paste::paste! {
            #[doc = $doc]
            #[allow(unused)]
            pub fn [<set_ $method>](&mut self) -> &mut Self {
                use bit_field::BitField;
                self.0.set_bit($bit, true);
                self
            }
        }
    };
}

macro_rules! clear_bit {
    ($bit:literal, $method:ident) => {
        paste::paste! {
            #[allow(unused)]
            pub fn [<clear_ $method>](&mut self) -> &mut Self {
                use bit_field::BitField;
                self.0.set_bit($bit, false);
                self
            }
        }
    };

    ($bit:literal, $method:ident, $doc:literal) => {
        paste::paste! {
            #[doc = $doc]
            #[allow(unused)]
            pub fn [<clear_ $method>](&mut self) -> &mut Self {
                use bit_field::BitField;
                self.0.set_bit($bit, false);
                self
            }
        }
    };
}

macro_rules! get_bit {
    ($bit:literal, $method:ident) => {
        paste::paste! {
            #[allow(unused)]
            pub fn [<get_ $method>](&mut self) -> bool {
                use bit_field::BitField;
                self.0.get_bit($bit)
            }
        }
    };

    ($bit:literal, $method:ident, $doc:literal) => {
        paste::paste! {
            #[doc = $doc]
            #[allow(unused)]
            pub fn [<get_ $method>](&mut self) -> bool {
                use bit_field::BitField;
                self.0.get_bit($bit)
            }
        }
    };
}

macro_rules! rw_bit {
    ($bit:literal, $method:ident) => {
        set_bit!($bit, $method);
        clear_bit!($bit, $method);
        get_bit!($bit, $method);
    };

    ($bit:literal, $method:ident, $doc:literal) => {
        set_bit!($bit, $method, $doc);
        clear_bit!($bit, $method, $doc);
        get_bit!($bit, $method, $doc);
    };
}

macro_rules! rw_bits {
    ($range:expr, $method:ident, $ty:ty) => {
        set_bits!($bit, $method, $ty);
        get_bits!($bit, $method, $ty);
    };

    ($range:expr, $method:ident, $ty:ty, $doc:literal) => {
        set_bits!($range, $method, $ty, $doc);
        get_bits!($range, $method, $ty, $doc);
    };
}

/// Prevent reallocation in initializing [`alloc::vec::Vec`]
macro_rules! vec_no_realloc {
    ($elem:expr; $capacity:expr; $alloc:expr) => {{
        extern crate alloc;
        use alloc::vec::Vec;

        let elem = $elem;
        let mut vector = Vec::with_capacity_in($capacity, $alloc);
        for _ in 0..$capacity {
            vector.push(elem.clone());
        }
        vector
    }};
}

macro_rules! vec_no_realloc_none {
    ($capacity:expr; $alloc:expr) => {{
        extern crate alloc;
        use alloc::vec::Vec;
        let mut vector = Vec::with_capacity_in($capacity, $alloc);
        for _ in 0..$capacity {
            vector.push(None);
        }
        vector
    }};
}

// This macro follows cascade crate (which is not maintained anymore)
// https://github.com/InquisitivePenguin/cascade/blob/03e8e820b3c05d0a5ad8a4e9386cee1015bb30b7/src/lib.rs
macro_rules! init_chain {
    ($init:expr; $tails:tt) => {
        init_chain!(let __init = $init; $tails)
    };

    (let $var:ident = $init:expr; $tails:tt) => {{
        let mut $var = $init;
        init_chain!(@__inner $var, $tails)
    }};

    // like init.method1(1, 2, 3).method2(4, 5)...
    // (@__inner $var:ident, <- $($method:ident ($($args:expr),* $(,)?)).+; $tails:tt) => {{
    (@__inner $var:ident, .. $method:tt; $tails:tt) => {{
        // $var.$($method($($args),*)).+;
        // $var.$method($($args),*);
        init_chain!(@__inner $var, $tails)
    }};

    // in case using shorthand for Result or Option
    (@__inner $var:ident, <- $($method:ident ($($args:expr),* $(,)?)).+?; $tails:tt) => {{
        $var.$($method($($args),*)).+?;
        init_chain!(@__inner $var, $tails)
    }};

    (@__inner $var:ident, $_yield:expr $(,)? ) => {
        $_yield
    };

    (@__inner $var:ident $(,)?) => {
        $var
    };

    () => {}
}

macro_rules! auto_repr_tryfrom {
    (
        $(#[$outer:meta])*
        $v:vis enum $name:ident : $uint:ty {
            $(
                $(#[$doc:meta])*
                $variant:ident = $value:literal
            ),* $(,)?
        }
        $(,)?
    ) => {
        #[repr($uint)]
        $(#[$outer])*
        $v enum $name {
            $(
                $(#[$doc])*
                $variant = $value,
            )*
        }
        impl From<$name> for $uint {
            fn from(value: $name) -> Self {
                match value {
                    $(
                        $name::$variant => $value,
                    )*
                }
            }
        }
        impl TryFrom<$uint> for $name {
            type Error = $uint;
            fn try_from(value: $uint) -> core::result::Result<Self, Self::Error> {
                match value {
                    $(
                        $value => Ok($name::$variant),
                    )*
                    _ => Err(value),
                }
            }
        }
    };
}
macro_rules! auto_unit_from {
    (
        $(#[$outer:meta])*
        $v:vis struct $name:ident($uint:ty) $(;)?
    ) => {
        $(#[$outer])*
        $v struct $name($uint);
        impl From<$name> for $uint {
            fn from(value: $name) -> Self {
                value.0
            }
        }
    };
}
