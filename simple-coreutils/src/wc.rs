use std::env;
use std::fs::File;
use std::io::{self, BufRead, BufReader, Read};

fn main() {
    let args: Vec<String> = env::args().collect();

    let mut show_lines = false;
    let mut show_words = false;
    let mut show_chars = false;
    let mut files: Vec<&str> = Vec::new();

    for arg in &args[1..] {
        match arg.as_str() {
            "-l" => show_lines = true,
            "-w" => show_words = true,
            "-c" | "-m" => show_chars = true,
            s if s.starts_with('-') => {
                for c in s.chars().skip(1) {
                    match c {
                        'l' => show_lines = true,
                        'w' => show_words = true,
                        'c' | 'm' => show_chars = true,
                        _ => {}
                    }
                }
            }
            s => files.push(s),
        }
    }

    // Default: show all
    if !show_lines && !show_words && !show_chars {
        show_lines = true;
        show_words = true;
        show_chars = true;
    }

    let mut total_lines = 0usize;
    let mut total_words = 0usize;
    let mut total_chars = 0usize;

    if files.is_empty() {
        let (l, w, c) = count_reader(io::stdin().lock());
        print_counts(l, w, c, "", show_lines, show_words, show_chars);
    } else {
        for path in &files {
            match File::open(path) {
                Ok(f) => {
                    let (l, w, c) = count_reader(BufReader::new(f));
                    print_counts(l, w, c, path, show_lines, show_words, show_chars);
                    total_lines += l;
                    total_words += w;
                    total_chars += c;
                }
                Err(e) => eprintln!("wc: {}: {}", path, e),
            }
        }
        if files.len() > 1 {
            print_counts(total_lines, total_words, total_chars, "total", show_lines, show_words, show_chars);
        }
    }
}

fn count_reader<R: BufRead>(reader: R) -> (usize, usize, usize) {
    let mut lines = 0;
    let mut words = 0;
    let mut chars = 0;

    for line in reader.lines().flatten() {
        lines += 1;
        chars += line.len() + 1; // +1 for newline
        words += line.split_whitespace().count();
    }

    (lines, words, chars)
}

fn print_counts(lines: usize, words: usize, chars: usize, name: &str, sl: bool, sw: bool, sc: bool) {
    let mut parts = Vec::new();
    if sl {
        parts.push(format!("{:8}", lines));
    }
    if sw {
        parts.push(format!("{:8}", words));
    }
    if sc {
        parts.push(format!("{:8}", chars));
    }
    if name.is_empty() {
        println!("{}", parts.join(""));
    } else {
        println!("{} {}", parts.join(""), name);
    }
}
