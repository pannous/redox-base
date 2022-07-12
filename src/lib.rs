#![no_std]
#![feature(alloc_error_handler, core_intrinsics, lang_items, panic_info_message)]

#[cfg(target_arch = "x86_64")]
pub mod x86_64;

pub mod exec;
pub mod initfs;

extern crate alloc;

#[panic_handler]
fn panic_handler(info: &core::panic::PanicInfo) -> ! {
    use core::fmt::Write;

    struct Writer;

    impl Write for Writer {
        fn write_str(&mut self, s: &str) -> core::fmt::Result {
            syscall::write(1, s.as_bytes()).map_err(|_| core::fmt::Error).map(|_| ())
        }
    }

    let _ = syscall::write(1, b"panic: ");
    if let Some(message) = info.message() {
        writeln!(&mut Writer, "{}", message).unwrap();
    } else {
        let _ = syscall::write(1, b"(explicit panic)\n");
    }
    core::intrinsics::abort();
}

#[alloc_error_handler]
fn alloc_error_handler(_: core::alloc::Layout) -> ! {
    core::intrinsics::abort();
}
#[lang = "eh_personality"]
extern "C" fn rust_eh_personality() {}

mod allocator {
    struct RelibcAllocator;

    #[global_allocator]
    static GLOBAL: RelibcAllocator = RelibcAllocator;

    use alloc::alloc::*;

    unsafe impl GlobalAlloc for RelibcAllocator {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            let mut ptr = core::ptr::null_mut();
            let align = core::cmp::max(layout.align(), core::mem::align_of::<*mut libc::c_void>());

            if libc::posix_memalign(&mut ptr, align, layout.size()) == 0 {
                ptr.cast()
            } else {
                core::ptr::null_mut()
            }
        }
        unsafe fn dealloc(&self, ptr: *mut u8, _layout: Layout) {
            libc::free(ptr.cast());
        }
    }
}
