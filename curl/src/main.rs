// Simple curl implementation for Redox OS
// Supports basic HTTP GET requests

use std::env;
use std::fs::File;
use std::io::{self, Read, Write};
use std::process;

fn print_usage() {
    eprintln!("Usage: curl [options] <url>");
    eprintln!("Options:");
    eprintln!("  -o <file>    Write output to file");
    eprintln!("  -O           Write to file named from URL");
    eprintln!("  -s           Silent mode");
    eprintln!("  -v           Verbose mode");
    eprintln!("  -I           Show headers only");
    eprintln!("  -L           Follow redirects");
}

fn filename_from_url(url: &str) -> Option<String> {
    url.rsplit('/').next()
        .filter(|s| !s.is_empty() && !s.contains('?'))
        .map(|s| s.to_string())
}

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();

    if args.is_empty() {
        print_usage();
        process::exit(1);
    }

    let mut url = None;
    let mut output_file = None;
    let mut silent = false;
    let mut verbose = false;
    let mut headers_only = false;
    let mut follow_redirects = false;
    let mut use_url_filename = false;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-o" => {
                i += 1;
                if i < args.len() {
                    output_file = Some(args[i].clone());
                }
            }
            "-O" => use_url_filename = true,
            "-s" => silent = true,
            "-v" => verbose = true,
            "-I" => headers_only = true,
            "-L" => follow_redirects = true,
            "-h" | "--help" => {
                print_usage();
                process::exit(0);
            }
            arg if !arg.starts_with('-') => url = Some(arg.to_string()),
            _ => {
                eprintln!("Unknown option: {}", args[i]);
                process::exit(1);
            }
        }
        i += 1;
    }

    let url = match url {
        Some(u) => u,
        None => {
            eprintln!("curl: no URL specified");
            process::exit(1);
        }
    };

    // Determine output filename from URL if -O
    if use_url_filename && output_file.is_none() {
        output_file = filename_from_url(&url);
        if output_file.is_none() {
            eprintln!("curl: cannot derive filename from URL");
            process::exit(1);
        }
    }

    if verbose {
        eprintln!("> GET {}", url);
    }

    // Build request
    let mut request = ureq::get(&url);

    // Make request
    let response = match request.call() {
        Ok(resp) => resp,
        Err(ureq::Error::Status(code, resp)) => {
            if !silent {
                eprintln!("curl: HTTP error {}", code);
            }
            if headers_only {
                for name in resp.headers_names() {
                    if let Some(value) = resp.header(&name) {
                        println!("{}: {}", name, value);
                    }
                }
            }
            process::exit(22);
        }
        Err(ureq::Error::Transport(e)) => {
            if !silent {
                eprintln!("curl: {}", e);
            }
            process::exit(6);
        }
    };

    let status = response.status();

    if verbose {
        eprintln!("< HTTP {} {}", status, response.status_text());
        for name in response.headers_names() {
            if let Some(value) = response.header(&name) {
                eprintln!("< {}: {}", name, value);
            }
        }
    }

    if headers_only {
        println!("HTTP/1.1 {} {}", status, response.status_text());
        for name in response.headers_names() {
            if let Some(value) = response.header(&name) {
                println!("{}: {}", name, value);
            }
        }
        return;
    }

    // Read body
    let mut reader = response.into_reader();
    let mut buffer = [0u8; 8192];

    match &output_file {
        Some(path) => {
            let mut file = match File::create(path) {
                Ok(f) => f,
                Err(e) => {
                    eprintln!("curl: cannot create '{}': {}", path, e);
                    process::exit(23);
                }
            };

            loop {
                match reader.read(&mut buffer) {
                    Ok(0) => break,
                    Ok(n) => {
                        if let Err(e) = file.write_all(&buffer[..n]) {
                            eprintln!("curl: write error: {}", e);
                            process::exit(23);
                        }
                    }
                    Err(e) => {
                        eprintln!("curl: read error: {}", e);
                        process::exit(56);
                    }
                }
            }

            if !silent {
                eprintln!("Saved to '{}'", path);
            }
        }
        None => {
            let stdout = io::stdout();
            let mut handle = stdout.lock();

            loop {
                match reader.read(&mut buffer) {
                    Ok(0) => break,
                    Ok(n) => {
                        if let Err(e) = handle.write_all(&buffer[..n]) {
                            eprintln!("curl: write error: {}", e);
                            process::exit(23);
                        }
                    }
                    Err(e) => {
                        eprintln!("curl: read error: {}", e);
                        process::exit(56);
                    }
                }
            }
        }
    }
}
