// Simple line editor for Redox OS
// Commands: p (print), a (append), i N (insert at line N), d N (delete line N), w (write), q (quit)

use std::env;
use std::fs::{File, OpenOptions};
use std::io::{self, BufRead, BufReader, Write, stdin, stdout};

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: simple-edit <filename>");
        std::process::exit(1);
    }

    let filename = &args[1];
    let mut lines: Vec<String> = Vec::new();
    let mut modified = false;

    // Try to read existing file
    if let Ok(file) = File::open(filename) {
        let reader = BufReader::new(file);
        for line in reader.lines() {
            if let Ok(l) = line {
                lines.push(l);
            }
        }
        println!("Loaded {} lines from {}", lines.len(), filename);
    } else {
        println!("New file: {}", filename);
    }

    println!("Commands: p[rint], a[ppend], i N [insert], d N [delete], w[rite], q[uit], h[elp]");

    let stdin = stdin();
    loop {
        print!("> ");
        let _ = stdout().flush();

        let mut input = String::new();
        if stdin.read_line(&mut input).is_err() {
            break;
        }
        let input = input.trim();

        if input.is_empty() {
            continue;
        }

        let parts: Vec<&str> = input.split_whitespace().collect();
        let cmd = parts[0];

        match cmd {
            "p" | "print" => {
                if lines.is_empty() {
                    println!("(empty)");
                } else {
                    for (i, line) in lines.iter().enumerate() {
                        println!("{:4}: {}", i + 1, line);
                    }
                }
            }
            "a" | "append" => {
                println!("Enter text (empty line to finish):");
                loop {
                    let mut line = String::new();
                    if stdin.read_line(&mut line).is_err() {
                        break;
                    }
                    let line = line.trim_end_matches('\n').to_string();
                    if line.is_empty() {
                        break;
                    }
                    lines.push(line);
                    modified = true;
                }
                println!("Now {} lines", lines.len());
            }
            "i" | "insert" => {
                if parts.len() < 2 {
                    println!("Usage: i <line_number>");
                    continue;
                }
                if let Ok(n) = parts[1].parse::<usize>() {
                    if n == 0 || n > lines.len() + 1 {
                        println!("Invalid line number (1-{})", lines.len() + 1);
                        continue;
                    }
                    println!("Enter text for line {}:", n);
                    let mut line = String::new();
                    if stdin.read_line(&mut line).is_ok() {
                        let line = line.trim_end_matches('\n').to_string();
                        lines.insert(n - 1, line);
                        modified = true;
                        println!("Inserted at line {}", n);
                    }
                }
            }
            "d" | "delete" => {
                if parts.len() < 2 {
                    println!("Usage: d <line_number>");
                    continue;
                }
                if let Ok(n) = parts[1].parse::<usize>() {
                    if n == 0 || n > lines.len() {
                        println!("Invalid line number (1-{})", lines.len());
                        continue;
                    }
                    lines.remove(n - 1);
                    modified = true;
                    println!("Deleted line {}", n);
                }
            }
            "e" | "edit" => {
                if parts.len() < 2 {
                    println!("Usage: e <line_number>");
                    continue;
                }
                if let Ok(n) = parts[1].parse::<usize>() {
                    if n == 0 || n > lines.len() {
                        println!("Invalid line number (1-{})", lines.len());
                        continue;
                    }
                    println!("Current: {}", lines[n - 1]);
                    println!("New text:");
                    let mut line = String::new();
                    if stdin.read_line(&mut line).is_ok() {
                        let line = line.trim_end_matches('\n').to_string();
                        lines[n - 1] = line;
                        modified = true;
                        println!("Updated line {}", n);
                    }
                }
            }
            "w" | "write" => {
                match File::create(filename) {
                    Ok(mut file) => {
                        for line in &lines {
                            if writeln!(file, "{}", line).is_err() {
                                println!("Error writing to file");
                                continue;
                            }
                        }
                        println!("Wrote {} lines to {}", lines.len(), filename);
                        modified = false;
                    }
                    Err(e) => println!("Cannot write: {}", e),
                }
            }
            "q" | "quit" => {
                if modified {
                    println!("Unsaved changes! Use 'q!' to quit without saving or 'w' to save.");
                } else {
                    break;
                }
            }
            "q!" => break,
            "wq" => {
                match File::create(filename) {
                    Ok(mut file) => {
                        for line in &lines {
                            let _ = writeln!(file, "{}", line);
                        }
                        println!("Wrote {} lines", lines.len());
                    }
                    Err(e) => println!("Cannot write: {}", e),
                }
                break;
            }
            "h" | "help" => {
                println!("Commands:");
                println!("  p        - print all lines");
                println!("  a        - append lines");
                println!("  i N      - insert before line N");
                println!("  e N      - edit line N");
                println!("  d N      - delete line N");
                println!("  w        - write file");
                println!("  q        - quit (warns if unsaved)");
                println!("  q!       - quit without saving");
                println!("  wq       - write and quit");
            }
            _ => println!("Unknown command. Type 'h' for help."),
        }
    }
}
