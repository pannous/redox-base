use std::fs;

fn main() {
    let path = std::env::args().nth(1).unwrap_or_else(|| ".".to_string());

    println!("Reading directory: {}", path);

    match fs::read_dir(&path) {
        Ok(entries) => {
            let mut files: Vec<_> = entries
                .filter_map(|e| e.ok())
                .collect();

            println!("Found {} entries", files.len());

            // Try to get metadata for each
            for entry in &files {
                let name = entry.file_name();
                print!("  {:?}: ", name);

                match entry.metadata() {
                    Ok(md) => {
                        match md.modified() {
                            Ok(time) => println!("mtime = {:?}", time),
                            Err(e) => println!("modified() error: {}", e),
                        }
                    }
                    Err(e) => println!("metadata error: {}", e),
                }
            }

            // Now try sorting
            println!("\nAttempting sort by mtime...");
            files.sort_by_key(|e| {
                e.metadata()
                    .and_then(|m| m.modified())
                    .ok()
            });
            println!("Sort succeeded!");

            for entry in &files {
                println!("  {:?}", entry.file_name());
            }
        }
        Err(e) => {
            println!("Error reading dir: {}", e);
        }
    }
}
