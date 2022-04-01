mod imp {
    include!(concat!(env!("OUT_DIR"), "/ascii.rs"));
}
// false positive
#[allow(unused_imports)]
pub use imp::*;

pub struct AsciiLayout {
    inner: &'static [u8],
}

impl AsciiLayout {
    pub fn as_slice(&self) -> &[u8] {
        self.inner
    }
}

pub fn get_font(c: char) -> AsciiLayout {
    let c = u8::try_from(u32::from(c)).unwrap_or(b'?') as usize;
    AsciiLayout {
        inner: &ASCII_FONT[c],
    }
}
