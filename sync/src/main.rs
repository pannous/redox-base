//! sync - flush filesystem buffers to disk
//!
//! In Redox's microkernel architecture, each filesystem handles its own caching.
//! This tool syncs files by opening and fsyncing key paths.

use std::env;
use std::fs::{self, File, OpenOptions};
use std::os::unix::io::AsRawFd;
use std::path::Path;

fn fsync_path(path: &Path) -> bool {
    if let Ok(f) = OpenOptions::new().read(true).open(path) {
        unsafe { libc::fsync(f.as_raw_fd()) == 0 }
    } else {
        false
    }
}

fn sync_dir(path: &Path) {
    if let Ok(entries) = fs::read_dir(path) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_file() {
                fsync_path(&p);
            }
        }
    }
    fsync_path(path);
}

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();

    if args.is_empty() {
        // Sync common locations
        for path in &["/", "/root", "/home", "/tmp"] {
            let p = Path::new(path);
            if p.exists() {
                sync_dir(p);
            }
        }
    } else {
        // Sync specified paths
        for arg in &args {
            let p = Path::new(arg);
            if p.is_dir() {
                sync_dir(p);
            } else {
                fsync_path(p);
            }
        }
    }
}
