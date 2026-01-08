// Simple rm implementation for Redox OS
use std::env;
use std::fs;
use std::path::Path;

fn remove_recursive(path: &Path) -> std::io::Result<()> {
    if path.is_dir() {
        for entry in fs::read_dir(path)? {
            let entry = entry?;
            remove_recursive(&entry.path())?;
        }
        fs::remove_dir(path)
    } else {
        fs::remove_file(path)
    }
}

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        eprintln!("rm: missing operand");
        std::process::exit(1);
    }

    let mut recursive = false;
    let mut force = false;
    let mut paths = Vec::new();

    for arg in &args[1..] {
        match arg.as_str() {
            "-r" | "-R" | "--recursive" => recursive = true,
            "-f" | "--force" => force = true,
            "-rf" | "-fr" => {
                recursive = true;
                force = true;
            }
            s if s.starts_with('-') => {
                // Handle combined flags like -rf
                for c in s.chars().skip(1) {
                    match c {
                        'r' | 'R' => recursive = true,
                        'f' => force = true,
                        _ => {
                            eprintln!("rm: invalid option -- '{}'", c);
                            std::process::exit(1);
                        }
                    }
                }
            }
            _ => paths.push(arg.clone()),
        }
    }

    if paths.is_empty() {
        eprintln!("rm: missing operand");
        std::process::exit(1);
    }

    let mut exit_code = 0;
    for path_str in &paths {
        let path = Path::new(path_str);

        if !path.exists() {
            if !force {
                eprintln!("rm: cannot remove '{}': No such file or directory", path_str);
                exit_code = 1;
            }
            continue;
        }

        let result = if path.is_dir() {
            if recursive {
                remove_recursive(path)
            } else {
                eprintln!("rm: cannot remove '{}': Is a directory", path_str);
                exit_code = 1;
                continue;
            }
        } else {
            fs::remove_file(path)
        };

        if let Err(e) = result {
            if !force {
                eprintln!("rm: cannot remove '{}': {}", path_str, e);
                exit_code = 1;
            }
        }
    }
    std::process::exit(exit_code);
}
