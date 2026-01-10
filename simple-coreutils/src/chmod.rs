use std::env;
use std::fs;
use std::os::unix::fs::PermissionsExt;

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() < 3 {
        eprintln!("usage: chmod MODE FILE...");
        std::process::exit(1);
    }

    let mode_str = &args[1];
    let mode = parse_mode(mode_str);

    if mode.is_none() {
        eprintln!("chmod: invalid mode '{}'", mode_str);
        std::process::exit(1);
    }

    let mode = mode.unwrap();
    let mut failed = false;

    for path in &args[2..] {
        if let Err(e) = set_permissions(path, mode) {
            eprintln!("chmod: {}: {}", path, e);
            failed = true;
        }
    }

    if failed {
        std::process::exit(1);
    }
}

fn parse_mode(s: &str) -> Option<u32> {
    // Try octal first (e.g., 755, 0644)
    if let Ok(mode) = u32::from_str_radix(s.trim_start_matches('0'), 8) {
        if mode <= 0o7777 {
            return Some(mode);
        }
    }

    // Simple symbolic mode support (e.g., +x, a+x, u+rwx)
    // For now, just handle common cases
    let s = s.trim();

    if s == "+x" || s == "a+x" {
        return Some(0o111); // Will be OR'd with existing
    }
    if s == "-x" || s == "a-x" {
        return Some(0o7666); // Will be AND'd
    }
    if s == "+r" || s == "a+r" {
        return Some(0o444);
    }
    if s == "+w" || s == "a+w" {
        return Some(0o222);
    }

    None
}

fn set_permissions(path: &str, mode: u32) -> std::io::Result<()> {
    let metadata = fs::metadata(path)?;
    let current_mode = metadata.permissions().mode();

    // For simple symbolic modes, we'd need to combine
    // For now, just set absolute mode for octal
    let new_mode = if mode <= 0o7777 {
        mode
    } else {
        current_mode & mode // For -x type operations
    };

    let permissions = fs::Permissions::from_mode(new_mode);
    fs::set_permissions(path, permissions)
}
