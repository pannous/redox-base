// POSIX-compatible file type detection for Redox OS
// Uses infer crate for magic number detection
use std::env;
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Read};
use std::os::unix::fs::FileTypeExt;
use std::path::Path;

const VERSION: &str = "1.0.0";

struct Options {
    brief: bool,
    mime_type: bool,
    mime_encoding: bool,
    follow_symlinks: bool,
    no_pad: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            brief: false,
            mime_type: false,
            mime_encoding: false,
            follow_symlinks: true,
            no_pad: false,
        }
    }
}

fn detect_file_type(path: &Path, opts: &Options) -> String {
    // Get metadata - follow symlinks based on -L/-h option
    let meta_result = if opts.follow_symlinks {
        fs::metadata(path)
    } else {
        fs::symlink_metadata(path)
    };

    // Check symlink first (only when not following)
    if !opts.follow_symlinks {
        if let Ok(meta) = fs::symlink_metadata(path) {
            if meta.file_type().is_symlink() {
                return match fs::read_link(path) {
                    Ok(target) => format!("symbolic link to {}", target.display()),
                    Err(_) => "symbolic link".to_string(),
                };
            }
        }
    }

    let metadata = match meta_result {
        Ok(m) => m,
        Err(e) => return format!("cannot open `{}' ({})", path.display(), e),
    };

    let ft = metadata.file_type();

    // Handle special file types
    if ft.is_dir() {
        return "directory".to_string();
    }
    if ft.is_symlink() {
        return match fs::read_link(path) {
            Ok(target) => format!("symbolic link to {}", target.display()),
            Err(_) => "symbolic link".to_string(),
        };
    }
    if ft.is_block_device() {
        return "block special".to_string();
    }
    if ft.is_char_device() {
        return "character special".to_string();
    }
    if ft.is_fifo() {
        return "fifo (named pipe)".to_string();
    }
    if ft.is_socket() {
        return "socket".to_string();
    }

    // Handle empty files
    if metadata.len() == 0 {
        return "empty".to_string();
    }

    // Read file header for magic detection
    let mut file = match File::open(path) {
        Ok(f) => f,
        Err(e) => return format!("cannot open `{}' ({})", path.display(), e),
    };

    let mut buffer = [0u8; 8192];
    let bytes_read = match file.read(&mut buffer) {
        Ok(n) => n,
        Err(e) => return format!("cannot read `{}' ({})", path.display(), e),
    };

    let buf = &buffer[..bytes_read];
    detect_content_type(buf, path)
}

fn detect_content_type(buf: &[u8], path: &Path) -> String {
    // Check ELF first for better output
    if buf.len() >= 4 && &buf[0..4] == b"\x7fELF" {
        return detect_elf_type(buf);
    }

    // Use infer for other binary formats
    if let Some(kind) = infer::get(buf) {
        return format_infer_type(kind);
    }

    // Fallback: check if it's text or binary
    if is_text(buf) {
        detect_text_type(buf, path)
    } else {
        "data".to_string()
    }
}

fn detect_elf_type(buf: &[u8]) -> String {
    if buf.len() < 20 {
        return "ELF".to_string();
    }

    let class = match buf[4] {
        1 => "32-bit",
        2 => "64-bit",
        _ => "",
    };

    let endian = match buf[5] {
        1 => "LSB",
        2 => "MSB",
        _ => "",
    };

    let etype = if buf.len() >= 18 {
        let et = if buf[5] == 1 {
            u16::from_le_bytes([buf[16], buf[17]])
        } else {
            u16::from_be_bytes([buf[16], buf[17]])
        };
        match et {
            1 => "relocatable",
            2 => "executable",
            3 => "shared object",
            4 => "core file",
            _ => "unknown",
        }
    } else {
        "unknown"
    };

    let machine = if buf.len() >= 20 {
        let em = if buf[5] == 1 {
            u16::from_le_bytes([buf[18], buf[19]])
        } else {
            u16::from_be_bytes([buf[18], buf[19]])
        };
        match em {
            3 => "Intel 80386",
            62 => "x86-64",
            183 => "ARM aarch64",
            40 => "ARM",
            8 => "MIPS",
            21 => "PowerPC64",
            20 => "PowerPC",
            43 => "SPARC V9",
            _ => "",
        }
    } else {
        ""
    };

    let mut desc = format!("ELF {} {} {}", class, endian, etype);
    if !machine.is_empty() {
        desc.push_str(", ");
        desc.push_str(machine);
    }
    desc
}

fn format_infer_type(kind: infer::Type) -> String {
    match kind.mime_type() {
        "application/gzip" => "gzip compressed data".to_string(),
        "application/zip" => "Zip archive data".to_string(),
        "application/x-tar" => "POSIX tar archive".to_string(),
        "application/x-bzip2" => "bzip2 compressed data".to_string(),
        "application/x-xz" => "XZ compressed data".to_string(),
        "application/x-7z-compressed" => "7-zip archive data".to_string(),
        "application/x-rar-compressed" => "RAR archive data".to_string(),
        "application/pdf" => "PDF document".to_string(),
        "application/x-sharedlib" | "application/x-executable" => {
            "ELF executable".to_string()
        }
        "image/png" => "PNG image data".to_string(),
        "image/jpeg" => "JPEG image data".to_string(),
        "image/gif" => "GIF image data".to_string(),
        "image/webp" => "WebP image data".to_string(),
        "image/bmp" => "BMP image data".to_string(),
        "image/tiff" => "TIFF image data".to_string(),
        "image/svg+xml" => "SVG image".to_string(),
        "audio/mpeg" => "MPEG audio".to_string(),
        "audio/ogg" => "Ogg audio".to_string(),
        "audio/flac" => "FLAC audio".to_string(),
        "audio/wav" => "RIFF WAVE audio".to_string(),
        "video/mp4" => "ISO Media, MP4".to_string(),
        "video/webm" => "WebM video".to_string(),
        "video/x-matroska" => "Matroska video".to_string(),
        "video/avi" => "RIFF AVI video".to_string(),
        "application/wasm" => "WebAssembly (wasm) binary module".to_string(),
        "font/woff" => "Web Open Font Format".to_string(),
        "font/woff2" => "Web Open Font Format 2".to_string(),
        _ => format!("{} data", kind.mime_type()),
    }
}

fn is_text(buf: &[u8]) -> bool {
    if buf.is_empty() {
        return true;
    }
    // Count printable/whitespace characters
    let text_chars = buf.iter().filter(|&&b| {
        b == 9 || b == 10 || b == 13 || (b >= 32 && b < 127)
    }).count();
    // Also allow UTF-8 continuation bytes
    let utf8_cont = buf.iter().filter(|&&b| b >= 128 && b < 192).count();
    (text_chars + utf8_cont) * 100 / buf.len() > 85
}

fn detect_text_type(buf: &[u8], path: &Path) -> String {
    let content = String::from_utf8_lossy(buf);
    let first_line = content.lines().next().unwrap_or("");

    // Check shebang
    if first_line.starts_with("#!") {
        let interp = first_line.trim_start_matches("#!");
        if interp.contains("python") {
            return "Python script, ASCII text executable".to_string();
        } else if interp.contains("bash") {
            return "Bourne-Again shell script, ASCII text executable".to_string();
        } else if interp.contains("/sh") {
            return "POSIX shell script, ASCII text executable".to_string();
        } else if interp.contains("perl") {
            return "Perl script, ASCII text executable".to_string();
        } else if interp.contains("ruby") {
            return "Ruby script, ASCII text executable".to_string();
        } else if interp.contains("node") || interp.contains("deno") {
            return "JavaScript script, ASCII text executable".to_string();
        } else if interp.contains("ion") {
            return "Ion shell script, ASCII text executable".to_string();
        }
        return "script, ASCII text executable".to_string();
    }

    // Check by extension first (more reliable than content heuristics)
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        match ext.to_lowercase().as_str() {
            "rs" => return "Rust source, ASCII text".to_string(),
            "c" => return "C source, ASCII text".to_string(),
            "h" => return "C header, ASCII text".to_string(),
            "cpp" | "cc" | "cxx" => return "C++ source, ASCII text".to_string(),
            "hpp" | "hxx" => return "C++ header, ASCII text".to_string(),
            "py" => return "Python script, ASCII text".to_string(),
            "sh" => return "POSIX shell script, ASCII text".to_string(),
            "bash" => return "Bourne-Again shell script, ASCII text".to_string(),
            "js" | "mjs" => return "JavaScript source, ASCII text".to_string(),
            "ts" | "mts" => return "TypeScript source, ASCII text".to_string(),
            "json" => return "JSON data".to_string(),
            "toml" => return "TOML configuration, ASCII text".to_string(),
            "yaml" | "yml" => return "YAML configuration, ASCII text".to_string(),
            "xml" => return "XML document, ASCII text".to_string(),
            "html" | "htm" => return "HTML document, ASCII text".to_string(),
            "css" => return "CSS stylesheet, ASCII text".to_string(),
            "md" | "markdown" => return "Markdown document, ASCII text".to_string(),
            "txt" => return "ASCII text".to_string(),
            "csv" => return "CSV data, ASCII text".to_string(),
            "svg" => return "SVG image, ASCII text".to_string(),
            "makefile" | "mk" => return "makefile script, ASCII text".to_string(),
            "dockerfile" => return "Dockerfile, ASCII text".to_string(),
            "rc" => return "run commands, ASCII text".to_string(),
            "conf" | "cfg" | "ini" => return "configuration file, ASCII text".to_string(),
            "log" => return "log file, ASCII text".to_string(),
            _ => {}
        }
    }

    // Check filename patterns
    let filename = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    match filename.to_lowercase().as_str() {
        "makefile" | "gnumakefile" => return "makefile script, ASCII text".to_string(),
        "dockerfile" => return "Dockerfile, ASCII text".to_string(),
        "cargo.toml" => return "Cargo manifest, ASCII text".to_string(),
        "cargo.lock" => return "Cargo lockfile, ASCII text".to_string(),
        ".gitignore" | ".gitattributes" => return "Git configuration, ASCII text".to_string(),
        _ => {}
    }

    // Content-based detection (fallback when extension doesn't match)
    if content.trim_start().starts_with("<?xml") {
        return "XML document, ASCII text".to_string();
    }
    if content.trim_start().starts_with("<!DOCTYPE html") ||
       content.trim_start().to_lowercase().starts_with("<html") {
        return "HTML document, ASCII text".to_string();
    }

    // Generic JSON detection
    let trimmed = content.trim_start();
    if trimmed.starts_with('{') || trimmed.starts_with('[') {
        if (trimmed.starts_with('{') && trimmed.contains(':')) ||
           (trimmed.starts_with('[') && (trimmed.contains(',') || trimmed.len() < 100)) {
            return "JSON data".to_string();
        }
    }

    "ASCII text".to_string()
}

fn get_mime_type(path: &Path, opts: &Options) -> String {
    let meta_result = if opts.follow_symlinks {
        fs::metadata(path)
    } else {
        fs::symlink_metadata(path)
    };

    let metadata = match meta_result {
        Ok(m) => m,
        Err(_) => return "application/octet-stream".to_string(),
    };

    let ft = metadata.file_type();

    if ft.is_dir() {
        return "inode/directory".to_string();
    }
    if ft.is_symlink() {
        return "inode/symlink".to_string();
    }
    if ft.is_block_device() {
        return "inode/blockdevice".to_string();
    }
    if ft.is_char_device() {
        return "inode/chardevice".to_string();
    }
    if ft.is_fifo() {
        return "inode/fifo".to_string();
    }
    if ft.is_socket() {
        return "inode/socket".to_string();
    }
    if metadata.len() == 0 {
        return "inode/x-empty".to_string();
    }

    let mut file = match File::open(path) {
        Ok(f) => f,
        Err(_) => return "application/octet-stream".to_string(),
    };

    let mut buffer = [0u8; 8192];
    let bytes_read = match file.read(&mut buffer) {
        Ok(n) => n,
        Err(_) => return "application/octet-stream".to_string(),
    };

    let buf = &buffer[..bytes_read];

    // Check ELF
    if buf.len() >= 4 && &buf[0..4] == b"\x7fELF" {
        return "application/x-executable".to_string();
    }

    if let Some(kind) = infer::get(buf) {
        return kind.mime_type().to_string();
    }

    if is_text(buf) {
        let mut mime = "text/plain".to_string();
        if opts.mime_encoding {
            mime.push_str("; charset=us-ascii");
        }
        mime
    } else {
        "application/octet-stream".to_string()
    }
}

fn print_usage() {
    eprintln!("Usage: file [-bchiLNv] [-f namefile] [file ...]");
    eprintln!("       file -v | --version");
    eprintln!("       file -h | --help");
}

fn print_help() {
    println!("Usage: file [OPTION...] [FILE...]");
    println!("Determine type of FILEs.\n");
    println!("Options:");
    println!("  -b, --brief         Do not prepend filenames to output lines");
    println!("  -c, --checking      (ignored, for compatibility)");
    println!("  -f, --files-from F  Read filenames from file F");
    println!("  -h, --no-dereference  Don't follow symlinks (default: follow)");
    println!("  -i, --mime          Output MIME type strings");
    println!("  -L, --dereference   Follow symlinks (default)");
    println!("  -N, --no-pad        Don't pad output");
    println!("      --mime-type     Output MIME type only");
    println!("      --mime-encoding Output MIME encoding only");
    println!("  -v, --version       Display version and exit");
    println!("      --help          Display this help and exit");
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let mut opts = Options::default();
    let mut files: Vec<String> = Vec::new();
    let mut files_from: Option<String> = None;
    let mut i = 1;

    while i < args.len() {
        let arg = &args[i];
        match arg.as_str() {
            "-b" | "--brief" => opts.brief = true,
            "-c" | "--checking-printout" => {} // ignored for compatibility
            "-h" | "--no-dereference" => opts.follow_symlinks = false,
            "-i" | "--mime" => {
                opts.mime_type = true;
                opts.mime_encoding = true;
            }
            "--mime-type" => opts.mime_type = true,
            "--mime-encoding" => opts.mime_encoding = true,
            "-L" | "--dereference" => opts.follow_symlinks = true,
            "-N" | "--no-pad" => opts.no_pad = true,
            "-v" | "--version" => {
                println!("file-{} (simple-file for Redox OS)", VERSION);
                println!("Using infer crate for magic detection");
                return;
            }
            "--help" => {
                print_help();
                return;
            }
            "-f" | "--files-from" => {
                i += 1;
                if i >= args.len() {
                    eprintln!("file: option requires an argument -- 'f'");
                    std::process::exit(1);
                }
                files_from = Some(args[i].clone());
            }
            _ if arg.starts_with('-') && arg.len() > 1 => {
                // Handle combined short options like -bL
                for c in arg.chars().skip(1) {
                    match c {
                        'b' => opts.brief = true,
                        'c' => {} // ignored
                        'h' => opts.follow_symlinks = false,
                        'i' => {
                            opts.mime_type = true;
                            opts.mime_encoding = true;
                        }
                        'L' => opts.follow_symlinks = true,
                        'N' => opts.no_pad = true,
                        'v' => {
                            println!("file-{}", VERSION);
                            return;
                        }
                        _ => {
                            eprintln!("file: invalid option -- '{}'", c);
                            print_usage();
                            std::process::exit(1);
                        }
                    }
                }
            }
            _ => files.push(arg.clone()),
        }
        i += 1;
    }

    // Read files from -f option
    if let Some(ref namefile) = files_from {
        match File::open(namefile) {
            Ok(f) => {
                let reader = BufReader::new(f);
                for line in reader.lines().map_while(Result::ok) {
                    let trimmed = line.trim();
                    if !trimmed.is_empty() {
                        files.push(trimmed.to_string());
                    }
                }
            }
            Err(e) => {
                eprintln!("file: cannot open `{}' ({})", namefile, e);
                std::process::exit(1);
            }
        }
    }

    if files.is_empty() {
        print_usage();
        std::process::exit(1);
    }

    // Calculate padding for aligned output
    let max_len = if opts.no_pad || opts.brief {
        0
    } else {
        files.iter().map(|f| f.len()).max().unwrap_or(0)
    };

    for file in &files {
        let path = Path::new(file);
        let result = if opts.mime_type {
            get_mime_type(path, &opts)
        } else {
            detect_file_type(path, &opts)
        };

        if opts.brief {
            println!("{}", result);
        } else if opts.no_pad || max_len == 0 {
            println!("{}: {}", file, result);
        } else {
            println!("{:width$} {}", format!("{}:", file), result, width = max_len + 1);
        }
    }
}
