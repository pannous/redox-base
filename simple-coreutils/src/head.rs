use std::env;
use std::fs::File;
use std::io::{self, BufRead, BufReader, Read};

fn main() {
    let args: Vec<String> = env::args().collect();

    let mut lines = 10usize;
    let mut files: Vec<&str> = Vec::new();

    let mut i = 1;
    while i < args.len() {
        let arg = &args[i];
        if arg == "-n" {
            i += 1;
            if i < args.len() {
                lines = args[i].parse().unwrap_or(10);
            }
        } else if arg.starts_with("-n") {
            lines = arg[2..].parse().unwrap_or(10);
        } else if arg.starts_with('-') && arg.chars().skip(1).all(|c| c.is_ascii_digit()) {
            lines = arg[1..].parse().unwrap_or(10);
        } else if arg.starts_with('-') {
            eprintln!("head: unknown option: {}", arg);
            std::process::exit(1);
        } else {
            files.push(arg);
        }
        i += 1;
    }

    if files.is_empty() {
        head_reader(io::stdin().lock(), lines);
    } else {
        let multiple = files.len() > 1;
        for (idx, path) in files.iter().enumerate() {
            if multiple {
                if idx > 0 {
                    println!();
                }
                println!("==> {} <==", path);
            }
            match File::open(path) {
                Ok(f) => head_reader(BufReader::new(f), lines),
                Err(e) => eprintln!("head: {}: {}", path, e),
            }
        }
    }
}

fn head_reader<R: BufRead>(reader: R, lines: usize) {
    for line in reader.lines().take(lines) {
        if let Ok(line) = line {
            println!("{}", line);
        }
    }
}
