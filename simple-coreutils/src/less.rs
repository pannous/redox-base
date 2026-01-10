// Simple less - wrapper that calls more for now
// A full less implementation would need bidirectional scrolling

use std::env;
use std::process::Command;

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();

    // For now, just delegate to more
    // A proper less would support scrolling back, searching, etc.
    let status = Command::new("more")
        .args(&args)
        .status();

    match status {
        Ok(s) => std::process::exit(s.code().unwrap_or(0)),
        Err(e) => {
            eprintln!("less: failed to execute more: {}", e);
            std::process::exit(1);
        }
    }
}
