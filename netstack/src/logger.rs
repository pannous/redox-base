use redox_log::{OutputBuilder, RedoxLogger};

pub fn init_logger(process_name: &str) {
    // Use WARN level by default to reduce noise. Set RUST_LOG=debug for verbose output.
    let log_level = std::env::var("RUST_LOG")
        .ok()
        .and_then(|s| s.parse::<log::LevelFilter>().ok())
        .unwrap_or(log::LevelFilter::Warn);

    if let Err(_) = RedoxLogger::new()
        .with_output(
            OutputBuilder::stdout()
                .with_ansi_escape_codes()
                .flush_on_newline(true)
                .with_filter(log_level)
                .build(),
        )
        .with_process_name(process_name.into())
        .enable()
    {
        eprintln!("{process_name}: Failed to init logger")
    }
}
