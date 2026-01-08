// Simple cp implementation for Redox OS
use std::env;
use std::fs;
use std::path::Path;

fn copy_recursive(src: &Path, dst: &Path) -> std::io::Result<()> {
    if src.is_dir() {
        fs::create_dir_all(dst)?;
        for entry in fs::read_dir(src)? {
            let entry = entry?;
            let src_path = entry.path();
            let dst_path = dst.join(entry.file_name());
            copy_recursive(&src_path, &dst_path)?;
        }
        Ok(())
    } else {
        if let Some(parent) = dst.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::copy(src, dst)?;
        Ok(())
    }
}

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() < 3 {
        eprintln!("cp: missing operand");
        eprintln!("Usage: cp [-r] SOURCE DEST");
        std::process::exit(1);
    }

    let mut recursive = false;
    let mut sources = Vec::new();

    for arg in &args[1..args.len()-1] {
        match arg.as_str() {
            "-r" | "-R" | "--recursive" => recursive = true,
            "-a" => recursive = true,  // -a implies -r
            s if s.starts_with('-') => {
                for c in s.chars().skip(1) {
                    if c == 'r' || c == 'R' {
                        recursive = true;
                    }
                }
            }
            _ => sources.push(arg.clone()),
        }
    }

    let dest = &args[args.len() - 1];
    let dest_path = Path::new(dest);

    if sources.is_empty() {
        eprintln!("cp: missing source operand");
        std::process::exit(1);
    }

    let mut exit_code = 0;

    // Multiple sources: destination must be directory
    if sources.len() > 1 && !dest_path.is_dir() {
        eprintln!("cp: target '{}' is not a directory", dest);
        std::process::exit(1);
    }

    for src_str in &sources {
        let src_path = Path::new(src_str);

        if !src_path.exists() {
            eprintln!("cp: cannot stat '{}': No such file or directory", src_str);
            exit_code = 1;
            continue;
        }

        let target = if dest_path.is_dir() {
            dest_path.join(src_path.file_name().unwrap_or_default())
        } else {
            dest_path.to_path_buf()
        };

        let result = if src_path.is_dir() {
            if recursive {
                copy_recursive(src_path, &target)
            } else {
                eprintln!("cp: -r not specified; omitting directory '{}'", src_str);
                exit_code = 1;
                continue;
            }
        } else {
            fs::copy(src_path, &target).map(|_| ())
        };

        if let Err(e) = result {
            eprintln!("cp: cannot copy '{}' to '{}': {}", src_str, target.display(), e);
            exit_code = 1;
        }
    }
    std::process::exit(exit_code);
}
