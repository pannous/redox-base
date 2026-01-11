//! Test file creation mode - enhanced debug version
use std::fs::{OpenOptions, metadata};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::os::unix::io::AsRawFd;

fn main() {
    println!("Testing file creation modes...\n");

    // Print umask first
    let old_umask = unsafe { libc::umask(0o022) };
    let _ = unsafe { libc::umask(old_umask) }; // restore
    println!("Current umask: 0o{:03o} (0x{:04x})", old_umask, old_umask);

    // Print flag values
    println!("\nFlag values:");
    println!("  O_WRONLY = 0x{:08x}", libc::O_WRONLY);
    println!("  O_CREAT  = 0x{:08x}", libc::O_CREAT);
    println!("  O_TRUNC  = 0x{:08x}", libc::O_TRUNC);

    // Calculate packed value
    let oflag: i32 = libc::O_WRONLY | libc::O_CREAT | libc::O_TRUNC;
    let mode: u32 = 0o644;
    let packed = ((oflag as usize) & 0xFFFF_0000) | ((mode as usize) & 0xFFFF);
    println!("\nPacked calculation:");
    println!("  oflag = 0x{:08x}", oflag);
    println!("  mode = 0o{:04o} (0x{:04x})", mode, mode);
    println!("  packed = 0x{:016x}", packed);
    println!("  packed as u16 = 0x{:04x} = 0o{:o}", packed as u16, packed as u16);
    println!();

    // Test 1: OpenOptions with explicit mode
    let path1 = "/tmp/test-mode-explicit.txt";
    let _ = std::fs::remove_file(path1);
    let f = OpenOptions::new()
        .write(true)
        .create(true)
        .mode(0o644)
        .open(path1);
    match f {
        Ok(file) => {
            let fd = file.as_raw_fd();

            // Get mode via Rust's metadata
            if let Ok(meta) = metadata(path1) {
                let mode = meta.permissions().mode();
                println!("Test 1 (explicit 0o644):");
                println!("  Rust mode = 0o{:o} (0x{:x})", mode & 0o7777, mode);
            }

            // Also try raw libc::fstat
            let mut stat_buf: libc::stat = unsafe { std::mem::zeroed() };
            let ret = unsafe { libc::fstat(fd, &mut stat_buf) };
            if ret == 0 {
                println!("  libc fstat mode = 0o{:o} (0x{:x})", stat_buf.st_mode as u32 & 0o7777, stat_buf.st_mode);
            } else {
                println!("  libc fstat failed: {}", ret);
            }
        }
        Err(e) => println!("Test 1 failed: {}", e),
    }

    // Test 2: Using libc::open directly with mode passed as 3rd arg
    let path2 = "/tmp/test-mode-libc.txt\0";
    let _ = std::fs::remove_file("/tmp/test-mode-libc.txt");
    unsafe {
        // Pass mode explicitly as mode_t (i32 on Redox)
        let fd = libc::open(
            path2.as_ptr() as *const libc::c_char,
            libc::O_WRONLY | libc::O_CREAT | libc::O_TRUNC,
            0o644 as libc::mode_t
        );
        if fd >= 0 {
            // Get mode via libc::fstat while fd is still open
            let mut stat_buf: libc::stat = std::mem::zeroed();
            let ret = libc::fstat(fd, &mut stat_buf);
            libc::close(fd);

            println!("\nTest 2 (libc::open 0o644):");
            if ret == 0 {
                println!("  libc fstat mode = 0o{:o} (0x{:x})", stat_buf.st_mode as u32 & 0o7777, stat_buf.st_mode);
            } else {
                println!("  libc fstat failed");
            }

            // Also via Rust metadata
            if let Ok(meta) = metadata("/tmp/test-mode-libc.txt") {
                let mode = meta.permissions().mode();
                println!("  Rust mode = 0o{:o} (0x{:x})", mode & 0o7777, mode);
            }
        } else {
            println!("\nTest 2 failed: fd = {}", fd);
        }
    }

    // Test 3: Try with a very different mode (0o755) to see pattern
    let path3 = "/tmp/test-mode-755.txt\0";
    let _ = std::fs::remove_file("/tmp/test-mode-755.txt");
    unsafe {
        let fd = libc::open(
            path3.as_ptr() as *const libc::c_char,
            libc::O_WRONLY | libc::O_CREAT | libc::O_TRUNC,
            0o755 as libc::mode_t
        );
        if fd >= 0 {
            let mut stat_buf: libc::stat = std::mem::zeroed();
            let ret = libc::fstat(fd, &mut stat_buf);
            libc::close(fd);

            println!("\nTest 3 (libc::open 0o755):");
            if ret == 0 {
                println!("  libc fstat mode = 0o{:o} (0x{:x})", stat_buf.st_mode as u32 & 0o7777, stat_buf.st_mode);
            }
        }
    }

    // Test 4: Very minimal mode (0o600)
    let path4 = "/tmp/test-mode-600.txt\0";
    let _ = std::fs::remove_file("/tmp/test-mode-600.txt");
    unsafe {
        let fd = libc::open(
            path4.as_ptr() as *const libc::c_char,
            libc::O_WRONLY | libc::O_CREAT | libc::O_TRUNC,
            0o600 as libc::mode_t
        );
        if fd >= 0 {
            let mut stat_buf: libc::stat = std::mem::zeroed();
            let ret = libc::fstat(fd, &mut stat_buf);
            libc::close(fd);

            println!("\nTest 4 (libc::open 0o600):");
            if ret == 0 {
                println!("  libc fstat mode = 0o{:o} (0x{:x})", stat_buf.st_mode as u32 & 0o7777, stat_buf.st_mode);
            }
        }
    }

    // Test 5: With umask=0 to eliminate umask as a factor
    let path5 = "/tmp/test-mode-no-umask.txt\0";
    let _ = std::fs::remove_file("/tmp/test-mode-no-umask.txt");
    unsafe {
        let old_mask = libc::umask(0); // Set umask to 0
        let fd = libc::open(
            path5.as_ptr() as *const libc::c_char,
            libc::O_WRONLY | libc::O_CREAT | libc::O_TRUNC,
            0o777 as libc::mode_t
        );
        libc::umask(old_mask); // Restore umask

        if fd >= 0 {
            let mut stat_buf: libc::stat = std::mem::zeroed();
            let ret = libc::fstat(fd, &mut stat_buf);
            libc::close(fd);

            println!("\nTest 5 (umask=0, mode=0o777):");
            if ret == 0 {
                println!("  libc fstat mode = 0o{:o} (0x{:x})", stat_buf.st_mode as u32 & 0o7777, stat_buf.st_mode);
                if stat_buf.st_mode as u32 & 0o7777 == 0o777 {
                    println!("  SUCCESS: Mode is correct!");
                } else {
                    println!("  FAIL: Mode is wrong (expected 0o777)");
                }
            }
        }
    }

    // Test 6: Use non-varargs __open_mode to bypass varargs issue
    println!("\nTest 6 (non-varargs __open_mode):");
    let path6 = "/tmp/test-mode-novarargs.txt\0";
    let _ = std::fs::remove_file("/tmp/test-mode-novarargs.txt");

    // Declare the non-varargs open function
    extern "C" {
        fn __open_mode(path: *const libc::c_char, oflag: libc::c_int, mode: libc::mode_t) -> libc::c_int;
    }

    unsafe {
        let fd = __open_mode(
            path6.as_ptr() as *const libc::c_char,
            libc::O_WRONLY | libc::O_CREAT | libc::O_TRUNC,
            0o644
        );
        if fd >= 0 {
            let mut stat_buf: libc::stat = std::mem::zeroed();
            let ret = libc::fstat(fd, &mut stat_buf);
            libc::close(fd);

            if ret == 0 {
                let got_mode = stat_buf.st_mode as u32 & 0o7777;
                println!("  mode = 0o{:o} (0x{:x})", got_mode, stat_buf.st_mode);
                if got_mode == 0o644 {
                    println!("  SUCCESS: Non-varargs __open_mode works correctly!");
                    println!("  The varargs calling convention is broken on aarch64 Cranelift.");
                } else {
                    println!("  FAIL: Mode still wrong. Issue is elsewhere.");
                }
            }
        } else {
            println!("  __open_mode not available (needs relibc rebuild)");
            println!("  HINT: Rebuild relibc with __open_mode function added");
        }
    }

    println!("\n=== Analysis ===");
    println!("If Test 6 succeeds, the fix is to use __open_mode instead of open()");
    println!("This requires patching Rust's libc crate to call __open_mode on Redox/aarch64");

    println!("\n=== Summary ===");
    println!("Bug: Cranelift varargs broken on aarch64");
    println!("Fix: Use non-varargs __open_mode function in relibc");
}
