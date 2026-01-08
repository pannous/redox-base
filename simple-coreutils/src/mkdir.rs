// Simple mkdir implementation for Redox OS
use std::env;
use std::fs;
use std::path::Path;

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        eprintln!("mkdir: missing operand");
        std::process::exit(1);
    }

    let mut parents = false;
    let mut mode: Option<u32> = None;
    let mut paths = Vec::new();

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "-p" | "--parents" => parents = true,
            "-m" | "--mode" => {
                i += 1;
                if i < args.len() {
                    mode = u32::from_str_radix(&args[i], 8).ok();
                }
            }
            s if s.starts_with("-m") => {
                mode = u32::from_str_radix(&s[2..], 8).ok();
            }
            s if s.starts_with('-') => {
                eprintln!("mkdir: unrecognized option '{}'", s);
                std::process::exit(1);
            }
            _ => paths.push(args[i].clone()),
        }
        i += 1;
    }

    if paths.is_empty() {
        eprintln!("mkdir: missing operand");
        std::process::exit(1);
    }

    let mut exit_code = 0;
    for path_str in &paths {
        let path = Path::new(path_str);

        let result = if parents {
            fs::create_dir_all(path)
        } else {
            fs::create_dir(path)
        };

        match result {
            Ok(_) => {
                // Set mode if specified (Unix only)
                #[cfg(unix)]
                if let Some(m) = mode {
                    use std::os::unix::fs::PermissionsExt;
                    let _ = fs::set_permissions(path, fs::Permissions::from_mode(m));
                }
            }
            Err(e) => {
                eprintln!("mkdir: cannot create directory '{}': {}", path_str, e);
                exit_code = 1;
            }
        }
    }
    std::process::exit(exit_code);
}
