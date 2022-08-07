use core::cell::UnsafeCell;
use core::fmt::Debug;
use core::mem::MaybeUninit;
use core::ops::{Index, IndexMut};
use core::ptr;

#[repr(transparent)]
pub struct VolatileCell<T> {
    value: UnsafeCell<MaybeUninit<T>>,
}

impl<T> VolatileCell<T> {
    pub const fn new(value: T) -> Self {
        Self {
            value: UnsafeCell::new(MaybeUninit::new(value)),
        }
    }

    pub fn write_volatile(&mut self, value: T) {
        unsafe { ptr::write_volatile(self.value.get(), MaybeUninit::new(value)) }
    }
}

impl<T: Copy> VolatileCell<T> {
    pub fn read_volatile(&self) -> T {
        unsafe { ptr::read_volatile(self.value.get() as *const MaybeUninit<T>).assume_init() }
    }

    pub fn update_volatile<F>(&mut self, f: F)
    where
        F: FnOnce(&mut T),
    {
        let mut value = self.read_volatile();
        f(&mut value);
        self.write_volatile(value);
    }
}

impl<T: Copy> Clone for VolatileCell<T> {
    fn clone(&self) -> Self {
        Self::new(unsafe { self.value.get().read().assume_init() })
    }
}

impl<T: Copy + Debug> Debug for VolatileCell<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("VolatileCell")
            .field("inner", &self.read_volatile())
            .finish()
    }
}

pub trait VolatileReadAt<T: Copy> {
    fn read_volatile_at(&self, i: usize) -> T;
}

pub trait VolatileWriteAt<T> {
    fn write_volatile_at(&mut self, i: usize, value: T);
}

pub trait VolatileUpdateAt<T: Copy>: VolatileWriteAt<T> + VolatileReadAt<T> {
    fn update_volatile_at<F>(&mut self, i: usize, f: F)
    where
        F: FnOnce(&mut T),
    {
        let mut value = self.read_volatile_at(i);
        f(&mut value);
        self.write_volatile_at(i, value);
    }
}

impl<T, V> VolatileUpdateAt<T> for V
where
    T: Copy,
    V: VolatileReadAt<T> + VolatileWriteAt<T>,
{
}

impl<T, ArrayType> VolatileReadAt<T> for ArrayType
where
    ArrayType: Index<usize, Output = VolatileCell<T>>,
    T: Copy,
{
    fn read_volatile_at(&self, i: usize) -> T {
        self[i].read_volatile()
    }
}

impl<T, ArrayType> VolatileWriteAt<T> for ArrayType
where
    ArrayType: IndexMut<usize, Output = VolatileCell<T>>,
{
    fn write_volatile_at(&mut self, i: usize, value: T) {
        self[i].write_volatile(value)
    }
}
