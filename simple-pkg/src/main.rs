// Simple package manager for Redox OS
// Uses ureq for HTTP (no async runtime issues)

use std::env;
use std::fs::{self, File};
use std::io::{self, Read, Write, BufReader};
use std::path::Path;
use std::process;

// HTTPS server requires TLS which we don't have, so use local packages
const PKG_SERVER: &str = "http://static.redox-os.org/pkg/aarch64-unknown-redox";
const PKG_DIR: &str = "/pkg";
const LOCAL_PKG: &str = "/scheme/9p.hostshare/packages";  // Host can put packages here

fn print_usage() {
    eprintln!("Redox Package Manager (simple-pkg)");
    eprintln!();
    eprintln!("Usage: pkg <command> [args]");
    eprintln!();
    eprintln!("Commands:");
    eprintln!("  list              List installed packages");
    eprintln!("  available         List packages in {}", LOCAL_PKG);
    eprintln!("  install <name>    Install package (from local or URL)");
    eprintln!("  install-local <path>  Install from local .tar.gz file");
    eprintln!("  search <name>     Search remote packages (requires HTTP)");
    eprintln!("  fetch <url>       Fetch and extract a package from URL");
    eprintln!();
    eprintln!("Note: Remote operations require HTTP (not HTTPS).");
    eprintln!("For HTTPS packages, download on host and place in:");
    eprintln!("  {}", LOCAL_PKG);
}

fn fetch_url(url: &str) -> Result<Vec<u8>, String> {
    eprintln!("Fetching: {}", url);

    let response = ureq::get(url)
        .call()
        .map_err(|e| format!("HTTP error: {}", e))?;

    let mut data = Vec::new();
    response.into_reader()
        .read_to_end(&mut data)
        .map_err(|e| format!("Read error: {}", e))?;

    Ok(data)
}

fn list_installed() {
    let pkg_dir = Path::new(PKG_DIR);

    if !pkg_dir.exists() {
        eprintln!("No packages installed (no {} directory)", PKG_DIR);
        return;
    }

    match fs::read_dir(pkg_dir) {
        Ok(entries) => {
            println!("Installed packages:");
            for entry in entries.flatten() {
                if entry.path().is_dir() {
                    println!("  {}", entry.file_name().to_string_lossy());
                }
            }
        }
        Err(e) => eprintln!("Error reading {}: {}", PKG_DIR, e),
    }
}

fn list_available() {
    let local_dir = Path::new(LOCAL_PKG);

    if !local_dir.exists() {
        eprintln!("Local package directory not found: {}", LOCAL_PKG);
        eprintln!("Create it on host at: /opt/other/redox/share/packages/");
        return;
    }

    match fs::read_dir(local_dir) {
        Ok(entries) => {
            println!("Available local packages:");
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.ends_with(".tar.gz") || name.ends_with(".tar") {
                    println!("  {}", name);
                }
            }
        }
        Err(e) => eprintln!("Error reading {}: {}", LOCAL_PKG, e),
    }
}

fn install_local(path: &str) {
    let path = if path.starts_with('/') {
        path.to_string()
    } else {
        format!("{}/{}", LOCAL_PKG, path)
    };

    if !Path::new(&path).exists() {
        eprintln!("File not found: {}", path);
        process::exit(1);
    }

    // Extract package name from filename
    let name = Path::new(&path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("package")
        .trim_end_matches(".tar");

    let dest_dir = format!("{}/{}", PKG_DIR, name);
    fs::create_dir_all(&dest_dir).ok();

    eprintln!("Installing {} from {}...", name, path);

    match extract_tar_gz(&path, &dest_dir) {
        Ok(_) => eprintln!("Successfully installed {}", name),
        Err(e) => eprintln!("Error extracting: {}", e),
    }
}

fn search_packages(query: &str) {
    let repo_url = format!("{}/repo.toml", PKG_SERVER);

    match fetch_url(&repo_url) {
        Ok(data) => {
            let content = String::from_utf8_lossy(&data);
            println!("Packages matching '{}':", query);

            for line in content.lines() {
                if line.starts_with('[') && line.ends_with(']') {
                    let name = &line[1..line.len()-1];
                    if name.contains(query) || query == "*" || query.is_empty() {
                        println!("  {}", name);
                    }
                }
            }
        }
        Err(e) => eprintln!("Error fetching repo: {}", e),
    }
}

fn install_package(name: &str) {
    // First try to get package info from repo.toml
    let repo_url = format!("{}/repo.toml", PKG_SERVER);

    let version = match fetch_url(&repo_url) {
        Ok(data) => {
            let content = String::from_utf8_lossy(&data);
            find_package_version(&content, name)
        }
        Err(_) => None,
    };

    let pkg_url = match version {
        Some(v) => format!("{}/{}/{}.tar.gz", PKG_SERVER, name, v),
        None => {
            // Try common version patterns
            eprintln!("Package version not found in repo, trying to fetch directly...");
            format!("{}/{}.tar.gz", PKG_SERVER, name)
        }
    };

    fetch_and_install(&pkg_url, name);
}

fn find_package_version(repo_content: &str, name: &str) -> Option<String> {
    let mut in_package = false;
    let target_header = format!("[{}]", name);

    for line in repo_content.lines() {
        if line.trim() == target_header {
            in_package = true;
            continue;
        }
        if in_package {
            if line.starts_with('[') {
                break; // Next package section
            }
            if line.starts_with("version") {
                // Parse: version = "1.2.3"
                if let Some(v) = line.split('=').nth(1) {
                    let v = v.trim().trim_matches('"').trim_matches('\'');
                    return Some(v.to_string());
                }
            }
        }
    }
    None
}

fn fetch_and_install(url: &str, name: &str) {
    eprintln!("Installing {} from {}", name, url);

    let data = match fetch_url(url) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("Error fetching package: {}", e);
            process::exit(1);
        }
    };

    eprintln!("Downloaded {} bytes", data.len());

    // Save to temp file
    let tmp_path = format!("/tmp/{}.tar.gz", name);
    if let Err(e) = fs::write(&tmp_path, &data) {
        eprintln!("Error saving package: {}", e);
        process::exit(1);
    }

    // Extract using tar crate
    let dest_dir = format!("{}/{}", PKG_DIR, name);
    fs::create_dir_all(&dest_dir).ok();

    eprintln!("Extracting to {}...", dest_dir);

    // For now, use command line tar if available, or implement extraction
    match extract_tar_gz(&tmp_path, &dest_dir) {
        Ok(_) => {
            eprintln!("Successfully installed {}", name);
            fs::remove_file(&tmp_path).ok();
        }
        Err(e) => {
            eprintln!("Error extracting: {}", e);
            eprintln!("Package saved to: {}", tmp_path);
        }
    }
}

fn extract_tar_gz(archive_path: &str, dest: &str) -> Result<(), String> {
    use std::io::BufReader;

    let file = File::open(archive_path)
        .map_err(|e| format!("Cannot open archive: {}", e))?;

    // Use flate2 for gzip if available, otherwise try raw tar
    // For simplicity, we'll try to use the tar crate directly
    // Note: This requires the file to be uncompressed or we need flate2

    // Try treating as plain tar first (many Redox packages are .tar not .tar.gz)
    let reader = BufReader::new(file);

    // The tar crate can handle this
    let mut archive = tar::Archive::new(reader);

    archive.unpack(dest)
        .map_err(|e| format!("Extraction failed: {}", e))?;

    Ok(())
}

fn show_info(name: &str) {
    let repo_url = format!("{}/repo.toml", PKG_SERVER);

    match fetch_url(&repo_url) {
        Ok(data) => {
            let content = String::from_utf8_lossy(&data);
            let mut in_package = false;
            let target_header = format!("[{}]", name);

            for line in content.lines() {
                if line.trim() == target_header {
                    in_package = true;
                    println!("Package: {}", name);
                    continue;
                }
                if in_package {
                    if line.starts_with('[') {
                        break;
                    }
                    if !line.trim().is_empty() {
                        println!("  {}", line.trim());
                    }
                }
            }

            if !in_package {
                eprintln!("Package '{}' not found", name);
            }
        }
        Err(e) => eprintln!("Error: {}", e),
    }
}

fn update_repo() {
    let repo_url = format!("{}/repo.toml", PKG_SERVER);

    match fetch_url(&repo_url) {
        Ok(data) => {
            let dest = format!("{}/repo.toml", PKG_DIR);
            fs::create_dir_all(PKG_DIR).ok();

            match fs::write(&dest, &data) {
                Ok(_) => eprintln!("Updated package list: {} bytes", data.len()),
                Err(e) => eprintln!("Error saving repo.toml: {}", e),
            }
        }
        Err(e) => eprintln!("Error fetching repo: {}", e),
    }
}

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        print_usage();
        process::exit(1);
    }

    match args[1].as_str() {
        "list" | "ls" => list_installed(),
        "available" | "avail" => list_available(),
        "search" | "find" => {
            let query = args.get(2).map(|s| s.as_str()).unwrap_or("*");
            search_packages(query);
        }
        "install" | "i" => {
            if args.len() < 3 {
                eprintln!("Usage: pkg install <package>");
                process::exit(1);
            }
            // Try local first, then remote
            let pkg = &args[2];
            let local_path = format!("{}/{}.tar.gz", LOCAL_PKG, pkg);
            if Path::new(&local_path).exists() {
                install_local(&local_path);
            } else {
                install_package(pkg);
            }
        }
        "install-local" | "il" => {
            if args.len() < 3 {
                eprintln!("Usage: pkg install-local <path>");
                process::exit(1);
            }
            install_local(&args[2]);
        }
        "info" | "show" => {
            if args.len() < 3 {
                eprintln!("Usage: pkg info <package>");
                process::exit(1);
            }
            show_info(&args[2]);
        }
        "fetch" => {
            if args.len() < 3 {
                eprintln!("Usage: pkg fetch <url>");
                process::exit(1);
            }
            fetch_and_install(&args[2], "fetched");
        }
        "update" | "up" => update_repo(),
        "-h" | "--help" | "help" => print_usage(),
        cmd => {
            eprintln!("Unknown command: {}", cmd);
            print_usage();
            process::exit(1);
        }
    }
}
