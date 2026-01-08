// Simple ls implementation for Redox OS
use std::env;
use std::fs;
use std::os::unix::fs::MetadataExt;
use std::path::Path;

fn main() {
    let args: Vec<String> = env::args().collect();

    // Parse flags
    let mut show_long = false;
    let mut show_all = false;
    let mut paths: Vec<&str> = Vec::new();

    for arg in &args[1..] {
        if arg.starts_with('-') {
            for c in arg.chars().skip(1) {
                match c {
                    'l' => show_long = true,
                    'a' => show_all = true,
                    _ => {}
                }
            }
        } else {
            paths.push(arg);
        }
    }

    if paths.is_empty() {
        paths.push(".");
    }

    for path in paths {
        list_path(path, show_long, show_all);
    }
}

fn list_path(path: &str, show_long: bool, show_all: bool) {
    let p = Path::new(path);

    // Handle single file
    if p.is_file() {
        if let Ok(meta) = fs::metadata(p) {
            if show_long {
                let mode = meta.mode();
                let size = meta.len();
                println!("-{:o} {:>8} {}", mode & 0o777, size, path);
            } else {
                println!("{}", path);
            }
        }
        return;
    }

    // Handle symlink pointing to file
    if p.is_symlink() {
        if let Ok(target) = fs::read_link(p) {
            if show_long {
                println!("l          {} -> {}", path, target.display());
            } else {
                println!("{}", path);
            }
        }
        return;
    }

    // Handle directory
    match fs::read_dir(path) {
        Ok(entries) => {
            for entry in entries {
                if let Ok(entry) = entry {
                    let name = entry.file_name();
                    let name_str = name.to_string_lossy();

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
        }
    }
}
