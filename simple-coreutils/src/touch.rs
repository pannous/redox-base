// Simple touch implementation for Redox OS
use std::env;
use std::fs::{File, OpenOptions};
use std::path::Path;

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        eprintln!("touch: missing file operand");
        std::process::exit(1);
    }

    let mut exit_code = 0;
    for path_str in &args[1..] {
        // Skip flags for now
        if path_str.starts_with('-') {
            continue;
        }

        let path = Path::new(path_str);

        let result = if path.exists() {
            // Update modification time by opening in append mode
            OpenOptions::new().append(true).open(path).map(|_| ())
        } else {
            // Create new empty file
            File::create(path).map(|_| ())
        };

        if let Err(e) = result {
            eprintln!("touch: cannot touch '{}': {}", path_str, e);
            exit_code = 1;
        }
    }
    std::process::exit(exit_code);
}
