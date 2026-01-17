// Simple HTTP/HTTPS client for Redox using std::net + rustls-rustcrypto
// Supports wget-like file download with -o FILE option
use std::env;
use std::fs::File;
use std::io::{self, Read, Write, BufRead, BufReader};
use std::net::TcpStream;
use std::process;
use std::sync::Arc;

use rustls::pki_types::ServerName;
use rustls::{ClientConfig, ClientConnection, StreamOwned, RootCertStore};

fn print_usage() {
    eprintln!("Usage: curl [options] <url>");
    eprintln!("Options:");
    eprintln!("  -o FILE      Write output to FILE (wget-style download)");
    eprintln!("  -O           Write to file named from URL");
    eprintln!("  -L           Follow redirects");
    eprintln!("  -v           Verbose mode");
    eprintln!("  -I           Show headers only");
    eprintln!("  -s           Silent mode (no progress)");
    eprintln!();
    eprintln!("Supports HTTP and HTTPS (pure-Rust TLS via rustls-rustcrypto).");
}

#[derive(Clone)]
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

struct Response {
    status_code: u16,
    headers: Vec<(String, String)>,
    content_length: Option<usize>,
    location: Option<String>,
}

fn do_request(
    stream: &mut dyn HttpStream,
    url: &UrlParts,
    headers_only: bool,
    verbose: bool,
    output: &mut dyn Write,
    show_progress: bool,
) -> io::Result<Response> {
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
        let _ = writeln!(output, "{}", line.trim_end());
    }

    // Parse status code
    let status_code: u16 = line
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);

    // Read headers
    let mut headers = Vec::new();
    let mut content_length = None;
    let mut location = None;

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
                }

                // Parse header
                if let Some((key, value)) = line.trim_end().split_once(':') {
                    let key = key.trim().to_lowercase();
                    let value = value.trim().to_string();

                    if key == "content-length" {
                        content_length = value.parse().ok();
                    } else if key == "location" {
                        location = Some(value.clone());
                    }

                    headers.push((key, value));
                }

                if verbose {
                    eprint!("< {}", line);
                } else if headers_only {
                    let _ = write!(output, "{}", line);
                }
            }
            Err(e) => return Err(e),
        }
    }

    if !headers_only && (status_code == 200 || status_code >= 400) {
        let mut buffer = [0u8; 8192];
        let mut total = 0usize;

        loop {
            match reader.read(&mut buffer) {
                Ok(0) => break,
                Ok(n) => {
                    output.write_all(&buffer[..n])?;
                    total += n;

                    if show_progress {
                        if let Some(len) = content_length {
                            let pct = (total * 100) / len;
                            eprint!("\r  {} / {} bytes ({}%)", total, len, pct);
                        } else {
                            eprint!("\r  {} bytes", total);
                        }
                    }
                }
                // Treat UnexpectedEof as normal EOF (server didn't send TLS close_notify)
                Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => break,
                Err(e) => return Err(e),
            }
        }

        if show_progress && total > 0 {
            eprintln!();
        }
    }

    Ok(Response {
        status_code,
        headers,
        content_length,
        location,
    })
}

fn fetch_url(
    url: &UrlParts,
    headers_only: bool,
    verbose: bool,
    output: &mut dyn Write,
    show_progress: bool,
) -> io::Result<Response> {
    let addr = format!("{}:{}", url.host, url.port);

    if verbose {
        eprintln!("* Connecting to {}...", addr);
    }

    let tcp_stream = TcpStream::connect(&addr).map_err(|e| {
        io::Error::new(e.kind(), format!("{}: Connection failed: {}", addr, e))
    })?;

    if verbose {
        eprintln!("* Connected to {} port {}", url.host, url.port);
    }

    if url.scheme == "https" {
        if verbose {
            eprintln!("* TLS handshake with {}...", url.host);
        }

        let tls_config = create_tls_config();
        let server_name = ServerName::try_from(url.host.clone())
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, format!("invalid server name: {}", e)))?;

        let tls_conn = ClientConnection::new(tls_config, server_name)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("TLS error: {}", e)))?;

        let mut tls_stream = StreamOwned::new(tls_conn, tcp_stream);

        if verbose {
            eprintln!("* TLS handshake complete");
        }

        do_request(&mut tls_stream, url, headers_only, verbose, output, show_progress)
    } else {
        let mut tcp = tcp_stream;
        do_request(&mut tcp, url, headers_only, verbose, output, show_progress)
    }
}

fn resolve_redirect(base_url: &UrlParts, location: &str) -> Option<UrlParts> {
    if location.starts_with("http://") || location.starts_with("https://") {
        // Absolute URL
        parse_url(location)
    } else if location.starts_with('/') {
        // Absolute path
        Some(UrlParts {
            scheme: base_url.scheme.clone(),
            host: base_url.host.clone(),
            port: base_url.port,
            path: location.to_string(),
        })
    } else {
        // Relative path (simple handling)
        let base_path = base_url.path.rsplit_once('/').map(|(p, _)| p).unwrap_or("");
        Some(UrlParts {
            scheme: base_url.scheme.clone(),
            host: base_url.host.clone(),
            port: base_url.port,
            path: format!("{}/{}", base_path, location),
        })
    }
}

fn main() {
    // Check if invoked as "wget" - if so, default to save-to-file mode
    let prog = env::args().next().unwrap_or_default();
    let wget_mode = prog.rsplit('/').next().unwrap_or("") == "wget";

    let args: Vec<String> = env::args().skip(1).collect();

    if args.is_empty() {
        print_usage();
        process::exit(1);
    }

    let mut url_str = None;
    let mut verbose = false;
    let mut headers_only = false;
    let mut follow_redirects = wget_mode;  // wget follows redirects by default
    let mut output_file: Option<String> = None;
    let mut remote_name = wget_mode;       // wget saves to file by default
    let mut silent = false;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-v" => verbose = true,
            "-I" => headers_only = true,
            "-L" => follow_redirects = true,
            "-s" => silent = true,
            "-O" => remote_name = true,
            "-o" => {
                i += 1;
                if i >= args.len() {
                    eprintln!("curl: -o requires a filename");
                    process::exit(1);
                }
                output_file = Some(args[i].clone());
            }
            "-h" | "--help" => {
                print_usage();
                process::exit(0);
            }
            s if !s.starts_with('-') => url_str = Some(s.to_string()),
            opt => {
                eprintln!("curl: unknown option: {}", opt);
                process::exit(1);
            }
        }
        i += 1;
    }

    let url_str = match url_str {
        Some(u) => u,
        None => {
            eprintln!("curl: no URL specified");
            process::exit(1);
        }
    };

    // Handle -O (remote name)
    if remote_name && output_file.is_none() {
        let filename = url_str
            .rsplit('/')
            .next()
            .unwrap_or("index.html")
            .split('?')
            .next()
            .unwrap_or("index.html");
        if filename.is_empty() {
            output_file = Some("index.html".to_string());
        } else {
            output_file = Some(filename.to_string());
        }
    }

    let mut url = match parse_url(&url_str) {
        Some(parts) => parts,
        None => {
            eprintln!("curl: invalid URL");
            process::exit(1);
        }
    };

    let show_progress = output_file.is_some() && !silent && !verbose;
    let max_redirects = 10;
    let mut redirects = 0;

    loop {
        if let Some(ref filename) = output_file {
            if !silent {
                eprintln!("  % Total    % Received");
            }
        }

        // Create output writer
        let result = if let Some(ref filename) = output_file {
            let mut file = match File::create(filename) {
                Ok(f) => f,
                Err(e) => {
                    eprintln!("curl: cannot create '{}': {}", filename, e);
                    process::exit(23);
                }
            };
            let res = fetch_url(&url, headers_only, verbose, &mut file, show_progress);
            if let Err(ref e) = res {
                eprintln!("curl: {}", e);
            }
            let _ = file.sync_all();
            res
        } else {
            let stdout = io::stdout();
            let mut handle = stdout.lock();
            fetch_url(&url, headers_only, verbose, &mut handle, false)
        };

        match result {
            Ok(response) => {
                // Check for redirects
                if follow_redirects && (response.status_code == 301 || response.status_code == 302 || response.status_code == 307 || response.status_code == 308) {
                    if let Some(location) = response.location {
                        redirects += 1;
                        if redirects > max_redirects {
                            eprintln!("curl: maximum redirects ({}) exceeded", max_redirects);
                            process::exit(47);
                        }

                        if verbose || !silent {
                            eprintln!("* Redirecting to: {}", location);
                        }

                        url = match resolve_redirect(&url, &location) {
                            Some(new_url) => new_url,
                            None => {
                                eprintln!("curl: invalid redirect location: {}", location);
                                process::exit(1);
                            }
                        };
                        continue;
                    }
                }

                // Success or non-redirect status
                if output_file.is_some() && !silent {
                    if let Some(len) = response.content_length {
                        eprintln!("  Downloaded {} bytes", len);
                    }
                }
                break;
            }
            Err(e) => {
                eprintln!("curl: {}", e);
                process::exit(56);
            }
        }
    }
}
