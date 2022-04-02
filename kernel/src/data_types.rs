use core::ops::{Div, DivAssign, Mul, MulAssign};
use derive_more::{Add, AddAssign, Neg, Sub, SubAssign};
use num_traits::NumAssign;

#[derive(Debug, Default, Clone, Copy, Add, AddAssign, Sub, SubAssign, Neg)]
pub struct Vec2D<T: NumAssign + core::marker::Copy> {
    pub x: T,
    pub y: T,
}

impl Vec2D<usize> {
    pub const fn zero() -> Self {
        Self { x: 0, y: 0 }
    }
}

impl<T: NumAssign + core::marker::Copy> Vec2D<T> {
    pub const fn new(x: T, y: T) -> Self {
        Self { x, y }
    }
}

impl<T: NumAssign + core::marker::Copy + core::marker::Copy> Mul<T> for Vec2D<T> {
    type Output = Self;
    fn mul(self, rhs: T) -> Self {
        Self {
            x: self.x * rhs,
            y: self.y * rhs,
        }
    }
}

impl<T: NumAssign + core::marker::Copy> MulAssign<T> for Vec2D<T> {
    fn mul_assign(&mut self, rhs: T) {
        self.x *= rhs;
        self.y *= rhs;
    }
}

impl<T: NumAssign + core::marker::Copy> Div<T> for Vec2D<T> {
    type Output = Self;
    fn div(self, rhs: T) -> Self {
        Self {
            x: self.x / rhs,
            y: self.y / rhs,
        }
    }
}

impl<T: NumAssign + core::marker::Copy> DivAssign<T> for Vec2D<T> {
    fn div_assign(&mut self, rhs: T) {
        self.x /= rhs
    }
}

impl<T: NumAssign + core::marker::Copy> From<(T, T)> for Vec2D<T> {
    fn from(from: (T, T)) -> Self {
        Self {
            x: from.0,
            y: from.1,
        }
    }
}
