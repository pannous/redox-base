use redox_log::{OutputBuilder, RedoxLogger};

pub fn init_logger(process_name: &str) {
    let mut logger = RedoxLogger::new()
        .with_output(
            OutputBuilder::stderr()
                .with_ansi_escape_codes()
                .flush_on_newline(true)
                .with_filter(log::LevelFilter::Info)
                .build(),
        )
        .with_process_name(process_name.into());

    // Also log to /scheme/logging/net/smolnetd.log
    #[cfg(target_os = "redox")]
    match OutputBuilder::in_redox_logging_scheme("net", "", "smolnetd.log") {
        Ok(b) => {
            logger = logger.with_output(
                b.with_filter(log::LevelFilter::Debug)
                    .flush_on_newline(true)
                    .build(),
            )
        }
        Err(error) => eprintln!("Failed to create smolnetd.log: {}", error),
    }

    if let Err(_) = logger.enable() {
        eprintln!("{process_name}: Failed to init logger")
    }
}
