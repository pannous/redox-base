use std::env;
use std::thread;
use std::time::Duration;

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        eprintln!("usage: sleep SECONDS");
        std::process::exit(1);
    }

    let mut total_secs: f64 = 0.0;

    for arg in &args[1..] {
        let secs = parse_duration(arg);
        if secs < 0.0 {
            eprintln!("sleep: invalid time interval '{}'", arg);
            std::process::exit(1);
        }
        total_secs += secs;
    }

    if total_secs > 0.0 {
        let dur = Duration::from_secs_f64(total_secs);
        thread::sleep(dur);
    }
}

fn parse_duration(s: &str) -> f64 {
    let s = s.trim();
    if s.is_empty() {
        return -1.0;
    }

    let (num_str, suffix) = if s.ends_with('s') || s.ends_with('S') {
        (&s[..s.len()-1], 1.0)
    } else if s.ends_with('m') || s.ends_with('M') {
        (&s[..s.len()-1], 60.0)
    } else if s.ends_with('h') || s.ends_with('H') {
        (&s[..s.len()-1], 3600.0)
    } else if s.ends_with('d') || s.ends_with('D') {
        (&s[..s.len()-1], 86400.0)
    } else {
        (s, 1.0)
    };

    match num_str.parse::<f64>() {
        Ok(n) if n >= 0.0 => n * suffix,
        _ => -1.0,
    }
}
