use core::alloc::Allocator;

/// Prevent reallocation in initializing [`alloc::vec::Vec`]
macro_rules! vec_no_realloc {
    ($elem:expr; $capacity:expr; $alloc:expr) => {{
        extern crate alloc;
        use alloc::vec::Vec;

        let elem = $elem;
        let mut vector = Vec::with_capacity_in($capacity, $alloc);
        for item in vector.iter_mut() {
            *item = elem.clone();
        }
        vector
    }};
}
