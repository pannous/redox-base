// Debug tool to print argv and environment info
use std::env;

fn main() {
    eprintln!("debug-argv: argc={}", env::args().count());
    for (i, arg) in env::args().enumerate() {
        eprintln!("debug-argv: argv[{}] = {:?}", i, arg);
    }

    if let Ok(path) = env::var("PATH") {
        eprintln!("debug-argv: PATH = {:?}", path);
    } else {
        eprintln!("debug-argv: PATH not set");
    }

    // Also print current exe path
    if let Ok(exe) = env::current_exe() {
        eprintln!("debug-argv: current_exe = {:?}", exe);
    }
}
