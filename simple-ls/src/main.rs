// Simple ls implementation for Redox OS
use std::env;
use std::fs;
use std::os::unix::fs::MetadataExt;

fn main() {
    let args: Vec<String> = env::args().collect();

    let path = if args.len() > 1 {
        &args[1]
    } else {
        "."
    };

    let show_long = args.iter().any(|a| a == "-l");
    let show_all = args.iter().any(|a| a == "-a");

    match fs::read_dir(path) {
        Ok(entries) => {
            for entry in entries {
                if let Ok(entry) = entry {
                    let name = entry.file_name();
                    let name_str = name.to_string_lossy();

                    // Skip hidden files unless -a
                    if !show_all && name_str.starts_with('.') {
                        continue;
                    }

                    if show_long {
                        if let Ok(meta) = entry.metadata() {
                            let file_type = if meta.is_dir() { "d" } else { "-" };
                            let mode = meta.mode();
                            let size = meta.len();
                            println!("{}{:o} {:>8} {}", file_type, mode & 0o777, size, name_str);
                        } else {
                            println!("???? {:>8} {}", "?", name_str);
                        }
                    } else {
                        print!("{}  ", name_str);
                    }
                }
            }
            if !show_long {
                println!();
            }
        }
        Err(e) => {
            eprintln!("ls: cannot access '{}': {}", path, e);
            std::process::exit(1);
        }
    }
}
