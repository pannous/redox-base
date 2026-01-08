// Simple echo implementation for Redox OS
use std::env;

fn main() {
    let args: Vec<String> = env::args().collect();

    let mut newline = true;
    let mut start = 1;

    // Check for -n flag
    if args.len() > 1 && args[1] == "-n" {
        newline = false;
        start = 2;
    }

    let output: String = args[start..].join(" ");

    if newline {
        println!("{}", output);
    } else {
        print!("{}", output);
    }
}
