// Simple test program to verify virtio-9p works

use std::fs;
use std::io::Read;
use std::process::Command;

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

    // Test executing simple-ls from initfs (Cranelift-compiled)
    let ls_path = "/scheme/initfs/bin/ls";
    eprintln!("test-9p: testing Cranelift-compiled ls from initfs");

    match fs::metadata(ls_path) {
        Ok(meta) => {
            eprintln!("test-9p: {} exists, size={}", ls_path, meta.len());
            eprintln!("test-9p: executing {} /scheme/", ls_path);
            match Command::new(ls_path).arg("/scheme/").output() {
                Ok(output) => {
                    eprintln!("test-9p: ls exit status: {:?}", output.status);
                    if !output.stdout.is_empty() {
                        eprintln!("test-9p: ls stdout:");
                        for line in String::from_utf8_lossy(&output.stdout).lines() {
                            eprintln!("  {}", line);
                        }
                    }
                    if !output.stderr.is_empty() {
                        eprintln!("test-9p: ls stderr: {}", String::from_utf8_lossy(&output.stderr));
                    }
                    if output.status.success() {
                        eprintln!("test-9p: CRANELIFT LS SUCCESS!");
                    }
                }
                Err(e) => eprintln!("test-9p: failed to execute ls: {}", e),
            }
        }
        Err(e) => eprintln!("test-9p: {} not found: {}", ls_path, e),
    }
}
