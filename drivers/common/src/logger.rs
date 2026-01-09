use redox_log::{OutputBuilder, RedoxLogger};

pub fn output_level() -> log::LevelFilter {
    // Check RUST_LOG env var, default to Info if not set
    match std::env::var("RUST_LOG").ok().as_deref() {
        Some("error") => log::LevelFilter::Error,
        Some("warn") => log::LevelFilter::Warn,
        Some("info") => log::LevelFilter::Info,
        Some("debug") => log::LevelFilter::Debug,
        Some("trace") => log::LevelFilter::Trace,
        Some("off") => log::LevelFilter::Off,
        _ => log::LevelFilter::Info, // default
    }
}

pub fn file_level() -> log::LevelFilter {
    // File logging always at Info level for debugging
    log::LevelFilter::Info
}

/// Configures logging for a single driver.
#[cfg_attr(not(target_os = "redox"), allow(unused_variables, unused_mut))]
pub fn setup_logging(
    category: &str,
    subcategory: &str,
    logfile_base: &str,
    output_level: log::LevelFilter,
    file_level: log::LevelFilter,
) {
    let mut logger = RedoxLogger::new().with_output(
        OutputBuilder::stderr()
            .with_filter(output_level) // limit global output to important info
            .with_ansi_escape_codes()
            .flush_on_newline(true)
            .build(),
    );

    #[cfg(target_os = "redox")]
    match OutputBuilder::in_redox_logging_scheme(
        category,
        subcategory,
        format!("{logfile_base}.log"),
    ) {
        Ok(b) => {
            logger = logger.with_output(b.with_filter(file_level).flush_on_newline(true).build())
        }
        Err(error) => eprintln!("Failed to create {logfile_base}.log: {}", error),
    }

    #[cfg(target_os = "redox")]
    match OutputBuilder::in_redox_logging_scheme(
        category,
        subcategory,
        format!("{logfile_base}.ansi.log"),
    ) {
        Ok(b) => {
            logger = logger.with_output(
                b.with_filter(file_level)
                    .with_ansi_escape_codes()
                    .flush_on_newline(true)
                    .build(),
            )
        }
        Err(error) => eprintln!("Failed to create {logfile_base}.ansi.log: {}", error),
    }

    logger.enable().expect("failed to set default logger");
}
