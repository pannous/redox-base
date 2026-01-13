// Simple HTTP client for Redox using std::net
use std::env;
use std::io::{self, Read, Write, BufRead, BufReader};
use std::net::TcpStream;
use std::process;
use std::time::Duration;

fn print_usage() {
    eprintln!("Usage: curl [options] <url>");
    eprintln!("Options:");
    eprintln!("  -v           Verbose mode");
    eprintln!("  -I           Show headers only");
}

fn parse_url(url: &str) -> Option<(String, String, String)> {
    // Parse http://host:port/path
    let url = url.strip_prefix("http://").unwrap_or(url);

    let (host_port, path) = url.split_once('/').unwrap_or((url, ""));
    let path = if path.is_empty() { "/" } else { &format!("/{}", path) };

    let (host, port) = if host_port.contains(':') {
        let mut parts = host_port.split(':');
        (parts.next()?.to_string(), parts.next()?.to_string())
    } else {
        (host_port.to_string(), "80".to_string())
    };

    Some((host, port, path.to_string()))
}

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();

    if args.is_empty() {
        print_usage();
        process::exit(1);
    }

    let mut url = None;
    let mut verbose = false;
    let mut headers_only = false;

    for arg in args.iter() {
        match arg.as_str() {
            "-v" => verbose = true,
            "-I" => headers_only = true,
            "-h" | "--help" => {
                print_usage();
                process::exit(0);
            }
            s if !s.starts_with('-') => url = Some(s.to_string()),
            _ => {
                eprintln!("Unknown option: {}", arg);
                process::exit(1);
            }
        }
    }

    let url = match url {
        Some(u) => u,
        None => {
            eprintln!("curl: no URL specified");
            process::exit(1);
        }
    };

    let (host, port, path) = match parse_url(&url) {
        Some(parts) => parts,
        None => {
            eprintln!("curl: invalid URL");
            process::exit(1);
        }
    };

    let addr = format!("{}:{}", host, port);

    if verbose {
        eprintln!("* Connecting to {}...", addr);
    }

    // Use connect() instead of connect_timeout() because connect() handles DNS resolution
    // via ToSocketAddrs trait, while connect_timeout() requires a pre-resolved SocketAddr
    let mut stream = match TcpStream::connect(&addr) {
        Ok(s) => {
            if verbose {
                eprintln!("* Connected to {} port {}", host, port);
            }
            s
        }
        Err(e) => {
            eprintln!("curl: {}: Connection failed: {}", url, e);
            process::exit(6);
        }
    };

    stream.set_read_timeout(Some(Duration::from_secs(30))).ok();
    stream.set_write_timeout(Some(Duration::from_secs(30))).ok();

    let request = if headers_only {
        format!("HEAD {} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\nUser-Agent: curl/redox\r\n\r\n", path, host)
    } else {
        format!("GET {} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\nUser-Agent: curl/redox\r\n\r\n", path, host)
    };

    if verbose {
        eprintln!("> {}", request.lines().next().unwrap());
        for line in request.lines().skip(1) {
            if !line.is_empty() {
                eprintln!("> {}", line);
            }
        }
    }

    if let Err(e) = stream.write_all(request.as_bytes()) {
        eprintln!("curl: write error: {}", e);
        process::exit(23);
    }

    if let Err(e) = stream.flush() {
        eprintln!("curl: flush error: {}", e);
        process::exit(23);
    }

    let mut reader = BufReader::new(stream);
    let mut line = String::new();

    // Read status line
    match reader.read_line(&mut line) {
        Ok(_) => {
            if verbose {
                eprint!("< {}", line);
            } else if headers_only {
                print!("{}", line);
            }
        }
        Err(e) => {
            eprintln!("curl: read error: {}", e);
            process::exit(56);
        }
    }

    // Read headers
    loop {
        line.clear();
        match reader.read_line(&mut line) {
            Ok(0) => break,
            Ok(_) => {
                if line == "\r\n" || line == "\n" {
                    if verbose {
                        eprintln!("<");
                    }
                    break;
                } else if verbose {
                    eprint!("< {}", line);
                } else if headers_only {
                    print!("{}", line);
                }
            }
            Err(e) => {
                eprintln!("curl: read error: {}", e);
                process::exit(56);
            }
        }
    }

    if !headers_only {
        // Read body
        let stdout = io::stdout();
        let mut handle = stdout.lock();
        let mut buffer = [0u8; 8192];

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
