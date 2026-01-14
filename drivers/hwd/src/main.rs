use std::process;
use std::time::Duration;

mod backend;

fn main() {
    // Minimal hwd - spawn acpid then pcid and exit
    // Skip daemon forking since it causes issues with Cranelift relibc

    eprintln!("hwd: starting minimal version");

    // Spawn acpid first - needed for PCI ECAM config on aarch64
    match process::Command::new("acpid").spawn() {
        Ok(_child) => {
            eprintln!("hwd: spawned acpid");
        }
        Err(err) => {
            eprintln!("hwd: failed to spawn acpid: {}", err);
        }
    }

    eprintln!("hwd: sleeping after acpid...");
    // Use a busy-wait loop instead of thread::sleep to avoid potential issues
    let start = std::time::Instant::now();
    while start.elapsed() < Duration::from_millis(500) {
        std::hint::spin_loop();
    }
    eprintln!("hwd: done sleeping");

    // Spawn pcid
    match process::Command::new("pcid").spawn() {
        Ok(_child) => {
            eprintln!("hwd: spawned pcid");
        }
        Err(err) => {
            eprintln!("hwd: failed to spawn pcid: {}", err);
        }
    }

    eprintln!("hwd: sleeping after pcid...");
    let start = std::time::Instant::now();
    while start.elapsed() < Duration::from_millis(500) {
        std::hint::spin_loop();
    }
    eprintln!("hwd: done");
}
