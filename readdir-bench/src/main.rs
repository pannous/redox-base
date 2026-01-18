use std::env;
use std::fs;
use std::time::Instant;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: readdir-bench <dir> [--stat]");
        return;
    }

    let dir = &args[1];
    let do_stat = args.get(2).map(|s| s == "--stat").unwrap_or(false);

    println!("Testing directory: {}", dir);
    println!("Mode: {}", if do_stat { "readdir + stat" } else { "readdir only" });

    let start = Instant::now();

    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(e) => {
            eprintln!("Error: {}", e);
            return;
        }
    };

    let readdir_time = start.elapsed();
    println!("read_dir() took: {:?}", readdir_time);

    let mut count = 0;
    let mut stat_count = 0;
    let iter_start = Instant::now();

    for entry in entries {
        if let Ok(entry) = entry {
            count += 1;
            if do_stat {
                if let Ok(_meta) = entry.metadata() {
                    stat_count += 1;
                }
            }
        }
    }

    let iter_time = iter_start.elapsed();
    let total_time = start.elapsed();

    println!("Iteration took: {:?}", iter_time);
    println!("Total entries: {}", count);
    if do_stat {
        println!("Stat succeeded: {}", stat_count);
        println!("Time per stat: {:?}", iter_time / stat_count as u32);
    }
    println!("Total time: {:?}", total_time);
    if count > 0 {
        println!("Time per entry: {:?}", total_time / count as u32);
    }
}
