#[macro_use]
pub mod console;
pub mod font;
pub mod frame_buffer;
mod paint;

pub use paint::*;
pub use frame_buffer::*;

use crate::data_types::Vec2D;
pub type Position = Vec2D<usize>;
pub type Offset = Vec2D<usize>;


