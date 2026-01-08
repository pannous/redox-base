// Simple cat implementation for Redox OS
use std::env;
use std::fs::File;
use std::io::{self, Read, Write};

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        // Read from stdin
        let mut buffer = Vec::new();
        if let Err(e) = io::stdin().read_to_end(&mut buffer) {
            eprintln!("cat: stdin: {}", e);
            std::process::exit(1);
        }
        let _ = io::stdout().write_all(&buffer);
        return;
    }

    let mut exit_code = 0;
    for path in &args[1..] {
        if path == "-" {
            let mut buffer = Vec::new();
            if let Err(e) = io::stdin().read_to_end(&mut buffer) {
                eprintln!("cat: stdin: {}", e);
                exit_code = 1;
                continue;
            }
            let _ = io::stdout().write_all(&buffer);
        } else {
            match File::open(path) {
                Ok(mut file) => {
                    let mut buffer = Vec::new();
                    match file.read_to_end(&mut buffer) {
                        Ok(_) => {
                            if let Err(e) = io::stdout().write_all(&buffer) {
                                eprintln!("cat: write error: {}", e);
                                exit_code = 1;
                            }
                        }
                        Err(e) => {
                            eprintln!("cat: {}: {}", path, e);
                            exit_code = 1;
                        }
                    }
                }
                Err(e) => {
                    eprintln!("cat: {}: {}", path, e);
                    exit_code = 1;
                }
            }
        }
    }
    std::process::exit(exit_code);
}
