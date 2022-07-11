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
