// Simple test program to verify virtio-9p works

use std::fs;
use std::io::Read;

fn main() {
    eprintln!("test-9p: starting");

    // List schemes
    eprintln!("test-9p: listing /scheme/");
    match fs::read_dir("/scheme") {
        Ok(entries) => {
            for entry in entries {
                if let Ok(e) = entry {
                    eprintln!("  {}", e.file_name().to_string_lossy());
                }
            }
        }
        Err(e) => eprintln!("test-9p: failed to list /scheme/: {}", e),
    }

    // Try to access 9p scheme
    let path = "/scheme/9p.hostshare/test.txt";
    eprintln!("test-9p: opening {}", path);

    match fs::File::open(path) {
        Ok(mut file) => {
            let mut contents = String::new();
            match file.read_to_string(&mut contents) {
                Ok(n) => {
                    eprintln!("test-9p: read {} bytes: {}", n, contents.trim());
                    eprintln!("test-9p: SUCCESS!");
                }
                Err(e) => eprintln!("test-9p: failed to read: {}", e),
            }
        }
        Err(e) => eprintln!("test-9p: failed to open: {}", e),
    }

    // Also try listing the 9p directory
    let dir = "/scheme/9p.hostshare/";
    eprintln!("test-9p: listing {}", dir);
    match fs::read_dir(dir) {
        Ok(entries) => {
            for entry in entries {
                if let Ok(e) = entry {
                    eprintln!("  {}", e.file_name().to_string_lossy());
                }
            }
        }
        Err(e) => eprintln!("test-9p: failed to list dir: {}", e),
    }
}
