//! A super simple initfs, only meant to be loaded into RAM by the bootloader, and then directly be
//! read.

pub mod types;

pub struct InitFs<'a> {
    base: &'a [u8],
}
