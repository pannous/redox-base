// Test cross-scheme symlink file operations
use std::fs::File;
use std::io::{Read, Write};

fn main() {
    println!("=== Cross-scheme symlink test ===");

    // Test 1: Read through cross-scheme symlink
    let read_path = "share/initrc";
    println!("\n1. Testing File::open on: {}", read_path);
    match File::open(read_path) {
        Ok(mut f) => {
            let mut contents = String::new();
            match f.read_to_string(&mut contents) {
                Ok(n) => println!("   SUCCESS: Read {} bytes: {:?}", n, &contents[..contents.len().min(50)]),
                Err(e) => println!("   ERROR reading: {}", e),
            }
        }
        Err(e) => println!("   ERROR opening: {}", e),
    }

    // Test 2: Write through cross-scheme symlink
    let write_path = "share/xscheme-test-output.txt";
    println!("\n2. Testing File::create on: {}", write_path);
    match File::create(write_path) {
        Ok(mut f) => {
            match f.write_all(b"Hello from cross-scheme write test!\n") {
                Ok(_) => println!("   SUCCESS: Wrote to {}", write_path),
                Err(e) => println!("   ERROR writing: {}", e),
            }
        }
        Err(e) => println!("   ERROR creating: {}", e),
    }

    // Test 3: Direct 9p path (should work)
    let direct_path = "/scheme/9p.hostshare/initrc";
    println!("\n3. Testing direct 9p path: {}", direct_path);
    match File::open(direct_path) {
        Ok(mut f) => {
            let mut contents = String::new();
            match f.read_to_string(&mut contents) {
                Ok(n) => println!("   SUCCESS: Read {} bytes", n),
                Err(e) => println!("   ERROR reading: {}", e),
            }
        }
        Err(e) => println!("   ERROR opening: {}", e),
    }

    println!("\n=== Test complete ===");
}
