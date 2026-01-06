use std::fs;
use std::process::Command;

use anyhow::{anyhow, Context, Result};

use pcid_interface::config::Config;
use pcid_interface::PciFunctionHandle;

fn busy_wait_ms(ms: u64) {
    // Use sched_yield syscall to actually wait - spin loops get optimized away
    // Each yield gives up timeslice to other processes
    let yields_per_ms = 10; // Approximate
    for _ in 0..(ms * yields_per_ms) {
        let _ = syscall::sched_yield();
    }
}

fn wait_for_scheme(path: &str, max_retries: u32, _delay_ms: u64) -> Result<fs::ReadDir> {
    // Wait up to 30 seconds with 100 retries of 300ms each
    for i in 0..100 {
        match fs::read_dir(path) {
            Ok(dir) => {
                eprintln!("pcid-spawner: found {} after {} attempts", path, i + 1);
                return Ok(dir);
            }
            Err(_e) => {
                if i % 10 == 0 {
                    eprintln!("pcid-spawner: waiting for {} (attempt {}/100)", path, i + 1);
                }
                busy_wait_ms(300); // 300ms per attempt
            }
        }
    }
    eprintln!("pcid-spawner: gave up waiting for {} after 100 attempts (30s)", path);
    Err(anyhow::anyhow!("timeout waiting for {}", path))
}

fn main() -> Result<()> {
    eprintln!("pcid-spawner: starting");

    let mut args = pico_args::Arguments::from_env();
    let config_path = args
        .free_from_str::<String>()
        .expect("failed to parse --config argument");

    eprintln!("pcid-spawner: config_path={}", config_path);

    common::setup_logging(
        "bus",
        "pci",
        "pci-spawner.log",
        common::output_level(),
        common::file_level(),
    );

    eprintln!("pcid-spawner: checking config file");
    let config_data = if fs::metadata(&config_path)?.is_file() {
        fs::read_to_string(&config_path)?
    } else {
        let mut config_data = String::new();
        for path in fs::read_dir(&config_path)? {
            if let Ok(tmp) = fs::read_to_string(path.unwrap().path()) {
                config_data.push_str(&tmp);
            }
        }
        config_data
    };
    eprintln!("pcid-spawner: parsing config");
    let config: Config = toml::from_str(&config_data)?;

    eprintln!("pcid-spawner: waiting for /scheme/pci");
    // Wait for pcid to register the pci scheme (workaround for race condition)
    for entry in wait_for_scheme("/scheme/pci", 50, 100)? {
        let entry = entry.context("failed to get entry")?;
        let device_path = entry.path();
        log::trace!("ENTRY: {}", device_path.to_string_lossy());

        let mut handle = match PciFunctionHandle::connect_by_path(&device_path) {
            Ok(handle) => handle,
            Err(err) => {
                // Either the device is gone or it is already in-use by a driver.
                log::debug!(
                    "pcid-spawner: {} already in use: {err}",
                    device_path.display(),
                );
                continue;
            }
        };

        let full_device_id = handle.config().func.full_device_id;

        log::debug!(
            "pcid-spawner enumerated: PCI {} {}",
            handle.config().func.addr,
            full_device_id.display()
        );

        let Some(driver) = config
            .drivers
            .iter()
            .find(|driver| driver.match_function(&full_device_id))
        else {
            log::debug!("no driver for {}, continuing", handle.config().func.addr);
            continue;
        };

        let mut args = driver.command.iter();

        let program = args
            .next()
            .ok_or_else(|| anyhow!("driver configuration entry did not have any command!"))?;
        let program = if program.starts_with('/') {
            program.to_owned()
        } else {
            "/usr/lib/drivers/".to_owned() + program
        };

        let mut command = Command::new(program);
        command.args(args);

        log::info!("pcid-spawner: spawn {:?}", command);

        handle.enable_device();

        let channel_fd = handle.into_inner_fd();
        command.env("PCID_CLIENT_CHANNEL", channel_fd.to_string());

        match command.status() {
            Ok(status) if !status.success() => {
                log::error!("pcid-spawner: driver {command:?} failed with {status}");
            }
            Ok(_) => {}
            Err(err) => log::error!("pcid-spawner: failed to execute {command:?}: {err}"),
        }
        syscall::close(channel_fd as usize).unwrap();
    }

    Ok(())
}
