#![no_std]
#![feature(alloc_error_handler, core_intrinsics, lang_items)]

#[cfg(target_arch = "x86_64")]
pub mod x86_64;

pub mod exec;

#[panic_handler]
fn panic_handler(_: &core::panic::PanicInfo) -> ! {
    core::intrinsics::abort();
}

#[alloc_error_handler]
fn alloc_error_handler(_: core::alloc::Layout) -> ! {
    core::intrinsics::abort();
}
#[lang = "eh_personality"]
extern "C" fn rust_eh_personality() {}
