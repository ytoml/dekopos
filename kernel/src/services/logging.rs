use core::fmt::Write;
use log::Log;

use super::CONSOLE;
use crate::graphics::{Color, Console};

pub(super) fn logger_init(mut console: Console<'static>) {
    console.set_background_color(Color::BLUE);
    console.set_output_color(Color::WHITE);
    console.fill_screen();
    let _ = unsafe { CONSOLE.insert(console) };
    log::set_logger(&KernelLogger).unwrap();
    log::set_max_level(log::LevelFilter::Info);
}

struct KernelLogger;

impl Log for KernelLogger {
    fn enabled(&self, _metadata: &log::Metadata) -> bool {
        true
    }

    fn log(&self, record: &log::Record) {
        let console = unsafe { CONSOLE.as_mut().unwrap() };
        writeln!(console, "{}: {}", record.level(), record.args()).unwrap();
    }

    fn flush(&self) {}
}
