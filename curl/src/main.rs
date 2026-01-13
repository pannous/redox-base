// Simple HTTP/HTTPS client for Redox using std::net + rustls-rustcrypto
use std::env;
use std::io::{self, Read, Write, BufRead, BufReader};
use std::net::TcpStream;
use std::process;
use std::sync::Arc;

use rustls::pki_types::ServerName;
use rustls::{ClientConfig, ClientConnection, StreamOwned, RootCertStore};

fn print_usage() {
    eprintln!("Usage: curl [options] <url>");
    eprintln!("Options:");
    eprintln!("  -v           Verbose mode");
    eprintln!("  -I           Show headers only");
    eprintln!();
    eprintln!("Supports HTTP and HTTPS (pure-Rust TLS via rustls-rustcrypto).");
}

struct UrlParts {
    scheme: String,
    host: String,
    port: u16,
    path: String,
}

fn parse_url(url: &str) -> Option<UrlParts> {
    let (scheme, rest) = if url.starts_with("https://") {
        ("https", &url[8..])
    } else if url.starts_with("http://") {
        ("http", &url[7..])
    } else {
        ("http", url)
    };

    let (host_port, path) = rest.split_once('/').unwrap_or((rest, ""));
    let path = if path.is_empty() { "/".to_string() } else { format!("/{}", path) };

    let default_port = if scheme == "https" { 443 } else { 80 };

    let (host, port) = if host_port.contains(':') {
        let mut parts = host_port.split(':');
        let h = parts.next()?.to_string();
        let p: u16 = parts.next()?.parse().ok()?;
        (h, p)
    } else {
        (host_port.to_string(), default_port)
    };

    Some(UrlParts { scheme: scheme.to_string(), host, port, path })
}

fn create_tls_config() -> Arc<ClientConfig> {
    let crypto = Arc::new(rustls_rustcrypto::provider());
    let root_store = RootCertStore::from_iter(
        webpki_roots::TLS_SERVER_ROOTS.iter().cloned()
    );

    let config = ClientConfig::builder_with_provider(crypto)
        .with_safe_default_protocol_versions()
        .expect("TLS protocol versions")
        .with_root_certificates(root_store)
        .with_no_client_auth();

    Arc::new(config)
}

trait HttpStream: Read + Write {}
impl<T: Read + Write> HttpStream for T {}

fn do_request(
    stream: &mut dyn HttpStream,
    url: &UrlParts,
    headers_only: bool,
    verbose: bool,
) -> io::Result<()> {
    let method = if headers_only { "HEAD" } else { "GET" };
    let request = format!(
        "{} {} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\nUser-Agent: curl/redox\r\n\r\n",
        method, url.path, url.host
    );

    if verbose {
        eprintln!("> {} {} HTTP/1.1", method, url.path);
        eprintln!("> Host: {}", url.host);
        eprintln!("> Connection: close");
        eprintln!("> User-Agent: curl/redox");
        eprintln!(">");
    }

    stream.write_all(request.as_bytes())?;
    stream.flush()?;

    let mut reader = BufReader::new(stream);
    let mut line = String::new();

    // Read status line
    reader.read_line(&mut line)?;
    if verbose {
        eprint!("< {}", line);
    } else if headers_only {
        print!("{}", line);
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
            Err(e) => return Err(e),
        }
    }

    if !headers_only {
        let stdout = io::stdout();
        let mut handle = stdout.lock();
        let mut buffer = [0u8; 8192];

        loop {
            match reader.read(&mut buffer) {
                Ok(0) => break,
                Ok(n) => handle.write_all(&buffer[..n])?,
                Err(e) => return Err(e),
            }
        }
    }

    Ok(())
}

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();

    if args.is_empty() {
        print_usage();
        process::exit(1);
    }

    let mut url_str = None;
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
            s if !s.starts_with('-') => url_str = Some(s.to_string()),
            _ => {
                eprintln!("Unknown option: {}", arg);
                process::exit(1);
            }
        }
    }

    let url_str = match url_str {
        Some(u) => u,
        None => {
            eprintln!("curl: no URL specified");
            process::exit(1);
        }
    };

    let url = match parse_url(&url_str) {
        Some(parts) => parts,
        None => {
            eprintln!("curl: invalid URL");
            process::exit(1);
        }
    };

    let addr = format!("{}:{}", url.host, url.port);

    if verbose {
        eprintln!("* Connecting to {}...", addr);
    }

    let tcp_stream = match TcpStream::connect(&addr) {
        Ok(s) => {
            if verbose {
                eprintln!("* Connected to {} port {}", url.host, url.port);
            }
            s
        }
        Err(e) => {
            eprintln!("curl: {}: Connection failed: {}", url_str, e);
            process::exit(6);
        }
    };

    let result = if url.scheme == "https" {
        if verbose {
            eprintln!("* TLS handshake with {}...", url.host);
        }

        let tls_config = create_tls_config();
        let server_name = match ServerName::try_from(url.host.clone()) {
            Ok(name) => name,
            Err(e) => {
                eprintln!("curl: invalid server name '{}': {}", url.host, e);
                process::exit(1);
            }
        };

        let tls_conn = match ClientConnection::new(tls_config, server_name) {
            Ok(conn) => conn,
            Err(e) => {
                eprintln!("curl: TLS connection failed: {}", e);
                process::exit(35);
            }
        };

        let mut tls_stream = StreamOwned::new(tls_conn, tcp_stream);

        if verbose {
            eprintln!("* TLS handshake complete");
        }

        do_request(&mut tls_stream, &url, headers_only, verbose)
    } else {
        let mut tcp = tcp_stream;
        do_request(&mut tcp, &url, headers_only, verbose)
    };

    if let Err(e) = result {
        eprintln!("curl: {}", e);
        process::exit(56);
    }
}
