// Simple file type detection for Redox OS using infer crate
use std::env;
use std::fs::{self, File};
use std::io::Read;
use std::path::Path;

fn detect_file_type(path: &Path) -> String {
    // Check if path exists
    let metadata = match fs::metadata(path) {
        Ok(m) => m,
        Err(e) => return format!("cannot open: {}", e),
    };

    // Handle directories
    if metadata.is_dir() {
        return "directory".to_string();
    }

    // Handle symlinks
    if path.is_symlink() {
        if let Ok(target) = fs::read_link(path) {
            return format!("symbolic link to {}", target.display());
        }
        return "symbolic link".to_string();
    }

    // Handle empty files
    if metadata.len() == 0 {
        return "empty".to_string();
    }

    // Read file header for magic detection
    let mut file = match File::open(path) {
        Ok(f) => f,
        Err(e) => return format!("cannot read: {}", e),
    };

    let mut buffer = [0u8; 8192];
    let bytes_read = match file.read(&mut buffer) {
        Ok(n) => n,
        Err(e) => return format!("read error: {}", e),
    };

    let buf = &buffer[..bytes_read];

    // Use infer to detect file type
    if let Some(kind) = infer::get(buf) {
        return format!("{} ({})", kind.mime_type(), kind.extension());
    }

    // Fallback: check if it's text or binary
    if is_text(buf) {
        detect_text_type(buf, path)
    } else {
        "data".to_string()
    }
}

fn is_text(buf: &[u8]) -> bool {
    // Check if content appears to be text (ASCII/UTF-8)
    let text_chars = buf.iter().filter(|&&b| {
        b == 9 || b == 10 || b == 13 || (b >= 32 && b < 127) || b >= 128
    }).count();

    // If >85% of bytes are text-like, consider it text
    text_chars * 100 / buf.len().max(1) > 85
}

fn detect_text_type(buf: &[u8], path: &Path) -> String {
    let content = String::from_utf8_lossy(buf);
    let first_line = content.lines().next().unwrap_or("");

    // Check shebang
    if first_line.starts_with("#!") {
        if first_line.contains("python") {
            return "Python script, ASCII text executable".to_string();
        } else if first_line.contains("bash") || first_line.contains("/sh") {
            return "shell script, ASCII text executable".to_string();
        } else if first_line.contains("perl") {
            return "Perl script, ASCII text executable".to_string();
        } else if first_line.contains("ruby") {
            return "Ruby script, ASCII text executable".to_string();
        } else if first_line.contains("node") {
            return "Node.js script, ASCII text executable".to_string();
        }
        return "script, ASCII text executable".to_string();
    }

    // Check by extension
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        match ext.to_lowercase().as_str() {
            "rs" => return "Rust source, ASCII text".to_string(),
            "c" | "h" => return "C source, ASCII text".to_string(),
            "cpp" | "cc" | "cxx" | "hpp" => return "C++ source, ASCII text".to_string(),
            "py" => return "Python script, ASCII text".to_string(),
            "sh" => return "shell script, ASCII text".to_string(),
            "js" => return "JavaScript source, ASCII text".to_string(),
            "ts" => return "TypeScript source, ASCII text".to_string(),
            "json" => return "JSON text".to_string(),
            "toml" => return "TOML text".to_string(),
            "yaml" | "yml" => return "YAML text".to_string(),
            "xml" => return "XML text".to_string(),
            "html" | "htm" => return "HTML document, ASCII text".to_string(),
            "css" => return "CSS stylesheet, ASCII text".to_string(),
            "md" => return "Markdown text".to_string(),
            "txt" => return "ASCII text".to_string(),
            _ => {}
        }
    }

    // Check content patterns
    if content.contains("<?xml") {
        return "XML text".to_string();
    }
    if content.contains("<!DOCTYPE html") || content.contains("<html") {
        return "HTML document, ASCII text".to_string();
    }
    if first_line.starts_with('{') || first_line.starts_with('[') {
        return "JSON text".to_string();
    }

    "ASCII text".to_string()
}

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        eprintln!("Usage: file <file>...");
        std::process::exit(1);
    }

    let mut brief = false;
    let mut mime = false;
    let mut files: Vec<&str> = Vec::new();

    for arg in &args[1..] {
        match arg.as_str() {
            "-b" | "--brief" => brief = true,
            "-i" | "--mime" => mime = true,
            "-h" | "--help" => {
                println!("Usage: file [options] <file>...");
                println!("Options:");
                println!("  -b, --brief  Do not prepend filenames to output");
                println!("  -i, --mime   Output MIME type only");
                println!("  -h, --help   Show this help");
                return;
            }
            _ if !arg.starts_with('-') => files.push(arg),
            _ => eprintln!("file: unknown option: {}", arg),
        }
    }

    if files.is_empty() {
        eprintln!("Usage: file <file>...");
        std::process::exit(1);
    }

    for file in files {
        let path = Path::new(file);
        let result = if mime {
            get_mime_type(path)
        } else {
            detect_file_type(path)
        };

        if brief {
            println!("{}", result);
        } else {
            println!("{}: {}", file, result);
        }
    }
}

fn get_mime_type(path: &Path) -> String {
    let metadata = match fs::metadata(path) {
        Ok(m) => m,
        Err(_) => return "application/octet-stream".to_string(),
    };

    if metadata.is_dir() {
        return "inode/directory".to_string();
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

    if let Some(kind) = infer::get(&buffer[..bytes_read]) {
        return kind.mime_type().to_string();
    }

    if is_text(&buffer[..bytes_read]) {
        "text/plain".to_string()
    } else {
        "application/octet-stream".to_string()
    }
}
