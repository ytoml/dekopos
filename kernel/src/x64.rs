pub use x86_64::align_up;
/// re-export x86_64 crate.
pub use x86_64::instructions::hlt;
pub use x86_64::instructions::interrupts::{disable, enable_and_hlt};
pub use x86_64::registers::model_specific::Msr;
