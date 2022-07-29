use crate::devices::usb::class::OnDataReceived;

pub type Observer = fn(u8, u8, u8);

#[derive(Debug)]
pub struct Mouse {
    observer: Observer,
}

impl Mouse {
    pub const fn new(observer: Observer) -> Self {
        Self { observer }
    }

    pub fn notify_move(&self, buttons: u8, dx: u8, dy: u8) {
        (self.observer)(buttons, dx, dy);
    }
}
impl OnDataReceived for Mouse {
    const BUFSIZE: usize = 3;
    fn on_data_received(&mut self, buf: &[u8]) {
        let buttons = buf[0];
        let dx = buf[1];
        let dy = buf[2];
        self.notify_move(buttons, dx, dy);
    }
}
