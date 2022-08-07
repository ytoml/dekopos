use bitvec::prelude::*;

use crate::devices::usb::class::OnDataReceived;
use crate::devices::usb::mem::{UsbAllocator, Vec};

pub type Observer = fn(u8, u8, bool);

#[derive(Debug)]
pub struct Keyboard {
    observer: Observer,
    prev_buf: Vec<u8>, // to check status change
}

impl Keyboard {
    pub fn new(observer: Observer) -> Self {
        Self {
            observer,
            prev_buf: vec_no_realloc![0u8; Self::BUFSIZE; UsbAllocator],
        }
    }
    pub fn notify_push(&self, modifier: u8, keycode: u8, pressed: bool) {
        (self.observer)(modifier, keycode, pressed);
    }
}
impl OnDataReceived for Keyboard {
    const BUFSIZE: usize = 8;
    fn on_data_received(&mut self, buf: &[u8]) {
        debug_assert_eq!(buf.len(), self.prev_buf.len());
        let modifier = buf[0];
        let mut cur = bitarr![u64, Lsb0; 0; 256];
        let mut prev = bitarr![u64, Lsb0; 0; 256];
        for index in buf.iter().take(7 + 1).skip(2).map(|&i| i as usize) {
            *cur.get_mut(index).unwrap() = true;
            *prev.get_mut(index).unwrap() = true;
        }
        let changed = prev ^ cur;
        let pressed = prev & cur;
        for (keycode, (ch, pr)) in changed.iter().zip(pressed.iter()).enumerate() {
            if keycode > 0 && *ch {
                self.notify_push(modifier, keycode.try_into().unwrap(), *pr)
            }
        }
    }
}
