use core::prelude::rust_2021::test;
use literal_expander::lazy_ucs2z;

#[test]
fn hello_world1() {
    let msg: [u16; 14] = lazy_ucs2z!("Hello, World!",);
    dbg!(msg);
}

#[test]
fn hello_world2() {
    let msg: [u16; 17] = lazy_ucs2z!("Hello!\n", "World!\n");
    dbg!(msg);
}
