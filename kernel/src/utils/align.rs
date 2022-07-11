use core::ops::{Index, IndexMut, Range};

macro_rules! align_wrapper {
    ($wrapper:ident, $align:literal) => {
        #[repr(align($align))]
        #[derive(Debug)]
        pub struct $wrapper<T> {
            inner: T,
        }

        impl<T> $wrapper<T> {
            pub const fn new(inner: T) -> Self {
                Self { inner }
            }
        }

        impl<T> AsRef<T> for $wrapper<T> {
            fn as_ref(&self) -> &T {
                &self.inner
            }
        }

        impl<T> AsMut<T> for $wrapper<T> {
            fn as_mut(&mut self) -> &mut T {
                &mut self.inner
            }
        }

        impl<T: Index<usize>> Index<usize> for $wrapper<T> {
            type Output = T::Output;
            fn index(&self, index: usize) -> &Self::Output {
                &self.inner[index]
            }
        }

        impl<T: IndexMut<usize>> IndexMut<usize> for $wrapper<T> {
            fn index_mut(&mut self, index: usize) -> &mut Self::Output {
                &mut self.inner[index]
            }
        }

        impl<T: Index<Range<usize>>> Index<Range<usize>> for $wrapper<T> {
            type Output = T::Output;
            fn index(&self, index: Range<usize>) -> &Self::Output {
                &self.inner[index]
            }
        }

        impl<T: IndexMut<Range<usize>>> IndexMut<Range<usize>> for $wrapper<T> {
            fn index_mut(&mut self, index: Range<usize>) -> &mut Self::Output {
                &mut self.inner[index]
            }
        }
    };
}

align_wrapper!(PageAligned, 4096);
align_wrapper!(Aligned64, 64);
