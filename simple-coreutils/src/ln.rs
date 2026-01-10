use std::env;
use std::fs;
use std::os::unix::fs as unix_fs;

fn main() {
    let args: Vec<String> = env::args().collect();

    let mut symbolic = false;
    let mut force = false;
    let mut files: Vec<&str> = Vec::new();

    for arg in &args[1..] {
        match arg.as_str() {
            "-s" | "--symbolic" => symbolic = true,
            "-f" | "--force" => force = true,
            "-sf" | "-fs" => {
                symbolic = true;
                force = true;
            }
            s if s.starts_with('-') => {
                eprintln!("ln: unknown option: {}", s);
                std::process::exit(1);
            }
            s => files.push(s),
        }
    }

    if files.len() < 2 {
        eprintln!("usage: ln [-sf] TARGET LINK_NAME");
        std::process::exit(1);
    }

    let target = files[0];
    let link_name = files[1];

    if force {
        let _ = fs::remove_file(link_name);
    }

    let result = if symbolic {
        unix_fs::symlink(target, link_name)
    } else {
        fs::hard_link(target, link_name)
    };

    if let Err(e) = result {
        eprintln!("ln: {}: {}", link_name, e);
        std::process::exit(1);
    }
}
