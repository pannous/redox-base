// Simple pager - displays file contents one screen at a time
// Press space for next page, q to quit, enter for next line

use std::env;
use std::fs::File;
use std::io::{self, BufRead, BufReader, Read, Write};

fn get_terminal_size() -> (usize, usize) {
    // Try to get from environment or use defaults
    let cols = env::var("COLUMNS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(80);
    let rows = env::var("LINES")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(24);
    (cols, rows)
}

fn read_key() -> Option<char> {
    let mut buf = [0u8; 1];
    if io::stdin().read(&mut buf).ok()? > 0 {
        Some(buf[0] as char)
    } else {
        None
    }
}

fn set_raw_mode(enable: bool) {
    // On Redox, we try to set raw mode via termios-like interface
    // For simplicity, we'll just work without raw mode if unavailable
    if enable {
        // Attempt to disable echo and canonical mode
        let _ = std::process::Command::new("stty")
            .args(["-echo", "-icanon", "min", "1"])
            .status();
    } else {
        let _ = std::process::Command::new("stty")
            .args(["echo", "icanon"])
            .status();
    }
}

fn page_file<R: BufRead>(reader: R, filename: Option<&str>) -> io::Result<bool> {
    let (cols, rows) = get_terminal_size();
    let page_size = rows.saturating_sub(1); // Leave room for prompt
    let mut line_count = 0;
    let mut stdout = io::stdout();

    if let Some(name) = filename {
        if name != "-" {
            println!("==> {} <==", name);
            line_count += 1;
        }
    }

    for line in reader.lines() {
        let line = line?;

        // Handle long lines by counting wrapped lines
        let display_lines = (line.len() / cols).max(1);

        println!("{}", line);
        line_count += display_lines;

        if line_count >= page_size {
            // Show prompt and wait for input
            print!("--More--");
            stdout.flush()?;

            set_raw_mode(true);
            let key = read_key();
            set_raw_mode(false);

            // Clear the prompt
            print!("\r        \r");
            stdout.flush()?;

            match key {
                Some('q') | Some('Q') => return Ok(false), // quit
                Some(' ') => line_count = 0,               // next page
                Some('\n') | Some('\r') => line_count = page_size - 1, // next line
                _ => line_count = 0,                       // default: next page
            }
        }
    }

    Ok(true) // continue to next file
}

fn main() -> io::Result<()> {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        // Read from stdin
        let stdin = io::stdin();
        let reader = BufReader::new(stdin.lock());
        page_file(reader, None)?;
    } else {
        // Process files
        let files: Vec<&str> = args[1..].iter().map(|s| s.as_str()).collect();
        let multiple = files.len() > 1;

        for (i, filename) in files.iter().enumerate() {
            if *filename == "-" {
                let stdin = io::stdin();
                let reader = BufReader::new(stdin.lock());
                if !page_file(reader, if multiple { Some(filename) } else { None })? {
                    break;
                }
            } else {
                match File::open(filename) {
                    Ok(file) => {
                        let reader = BufReader::new(file);
                        let show_name = if multiple { Some(*filename) } else { None };
                        if !page_file(reader, show_name)? {
                            break;
                        }
                        // Add blank line between files
                        if multiple && i < files.len() - 1 {
                            println!();
                        }
                    }
                    Err(e) => {
                        eprintln!("more: {}: {}", filename, e);
                    }
                }
            }
        }
    }

    Ok(())
}
