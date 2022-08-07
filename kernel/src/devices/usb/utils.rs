extern crate alloc;
use alloc::boxed::Box;

use core::alloc::Allocator;
use core::pin::Pin;

/// Leak raw pointer while wrapping value with [`Pin`]
pub fn leak_raw_pin<T, A>(value: T, alloc: A) -> (Pin<Box<T, A>>, *mut T)
where
    T: Unpin,
    A: 'static + Allocator + Clone,
{
    let pinned = Box::pin_in(value, alloc.clone());
    let raw = Box::into_raw(Pin::into_inner(pinned));
    let pinned = Pin::new(unsafe { Box::from_raw_in(raw, alloc) });
    (pinned, raw)
}

pub fn get_max_packet_size(port_speed_value: u8) -> u16 {
    match port_speed_value {
        1 => unimplemented!("get_max_packet_size: Full-Speed is out of scope."),
        2 => 8,
        3 => 64,
        4 => 512,
        psiv => {
            // TODO: Add support for psiv == 5 or 6.
            log::debug!("PSIV {psiv} is not expected, thus fall back to packet size = 8");
            8
        }
    }
}
