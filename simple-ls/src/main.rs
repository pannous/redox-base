// Simple ls implementation for Redox OS
use std::env;
use std::fs;
use std::os::unix::fs::MetadataExt;
use std::path::Path;

fn format_time(secs: i64) -> String {
    if secs == 0 {
        return "-".to_string();
    }
    // Simple timestamp formatting (YYYY-MM-DD HH:MM)
    const SECS_PER_MIN: i64 = 60;
    const SECS_PER_HOUR: i64 = 3600;
    const SECS_PER_DAY: i64 = 86400;

    let days_since_epoch = secs / SECS_PER_DAY;
    let time_of_day = secs % SECS_PER_DAY;

    let hour = time_of_day / SECS_PER_HOUR;
    let min = (time_of_day % SECS_PER_HOUR) / SECS_PER_MIN;

    // Calculate year/month/day from days since 1970-01-01
    let (year, month, day) = days_to_ymd(days_since_epoch);

    format!("{:04}-{:02}-{:02} {:02}:{:02}", year, month, day, hour, min)
}

fn days_to_ymd(mut days: i64) -> (i64, i64, i64) {
    let mut year = 1970;

    // Handle years
    loop {
        let days_in_year = if is_leap_year(year) { 366 } else { 365 };
        if days < days_in_year {
            break;
        }
        days -= days_in_year;
        year += 1;
    }

    // Handle months
    let days_in_month = if is_leap_year(year) {
        [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };

    let mut month = 1;
    for &d in &days_in_month {
        if days < d {
            break;
        }
        days -= d;
        month += 1;
    }

    (year, month, days + 1)
}

fn is_leap_year(year: i64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0)
}

fn main() {
    let args: Vec<String> = env::args().collect();

    // Parse flags
    let mut show_long = false;
    let mut show_all = false;
    let mut sort_by_time = false;
    let mut reverse_order = false;
    let mut paths: Vec<&str> = Vec::new();

    for arg in &args[1..] {
        if arg.starts_with('-') {
            for c in arg.chars().skip(1) {
                match c {
                    'l' => show_long = true,
                    'a' => show_all = true,
                    't' => sort_by_time = true,
                    'r' => reverse_order = true,
                    '1' => {} // One entry per line (implied with -l)
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
        list_path(path, show_long, show_all, sort_by_time, reverse_order);
    }
}

struct EntryInfo {
    name: String,
    is_dir: bool,
    is_symlink: bool,
    mode: u32,
    size: u64,
    mtime: i64,
    link_target: Option<String>,
}

fn list_path(path: &str, show_long: bool, show_all: bool, sort_by_time: bool, reverse_order: bool) {
    let p = Path::new(path);

    // Handle single file
    if p.is_file() {
        if let Ok(meta) = fs::metadata(p) {
            if show_long {
                let mode = meta.mode();
                let size = meta.len();
                let mtime = format_time(meta.mtime());
                println!("-{:o} {:>8} {} {}", mode & 0o777, size, mtime, path);
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

    // Handle directory - collect entries first for sorting
    match fs::read_dir(path) {
        Ok(entries) => {
            let mut entry_list: Vec<EntryInfo> = Vec::new();

            for entry in entries {
                if let Ok(entry) = entry {
                    let name = entry.file_name();
                    let name_str = name.to_string_lossy().to_string();

                    if !show_all && name_str.starts_with('.') {
                        continue;
                    }

                    let (is_dir, is_symlink, mode, size, mtime, link_target) =
                        if let Ok(meta) = entry.metadata() {
                            let symlink_meta = fs::symlink_metadata(entry.path()).ok();
                            let is_symlink = symlink_meta.map(|m| m.file_type().is_symlink()).unwrap_or(false);
                            let link_target = if is_symlink {
                                fs::read_link(entry.path()).ok().map(|p| p.display().to_string())
                            } else {
                                None
                            };
                            (meta.is_dir(), is_symlink, meta.mode(), meta.len(), meta.mtime(), link_target)
                        } else {
                            (false, false, 0, 0, 0, None)
                        };

                    entry_list.push(EntryInfo {
                        name: name_str,
                        is_dir,
                        is_symlink,
                        mode,
                        size,
                        mtime,
                        link_target,
                    });
                }
            }

            // Sort entries
            if sort_by_time {
                // Sort by time, newest first (descending mtime)
                // Larger mtime values should come first
                entry_list.sort_by(|a, b| {
                    match b.mtime.cmp(&a.mtime) {
                        std::cmp::Ordering::Equal => a.name.cmp(&b.name),
                        other => other,
                    }
                });
            } else {
                entry_list.sort_by(|a, b| a.name.cmp(&b.name)); // Alphabetical
            }

            if reverse_order {
                entry_list.reverse();
            }

            // Display entries
            for entry in &entry_list {
                if show_long {
                    let file_type = if entry.is_symlink {
                        "l"
                    } else if entry.is_dir {
                        "d"
                    } else {
                        "-"
                    };
                    let mtime_str = format_time(entry.mtime);
                    if let Some(ref target) = entry.link_target {
                        println!("{}{:o} {:>8} {} {} -> {}", file_type, entry.mode & 0o777, entry.size, mtime_str, entry.name, target);
                    } else {
                        println!("{}{:o} {:>8} {} {}", file_type, entry.mode & 0o777, entry.size, mtime_str, entry.name);
                    }
                } else {
                    print!("{}  ", entry.name);
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
