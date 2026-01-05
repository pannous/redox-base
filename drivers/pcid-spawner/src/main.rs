use std::fs;
use std::process::Command;
use std::thread;
use std::time::Duration;

use anyhow::{anyhow, Context, Result};

use pcid_interface::config::Config;
use pcid_interface::PciFunctionHandle;

fn wait_for_scheme(path: &str, max_retries: u32, delay_ms: u64) -> Result<fs::ReadDir> {
    for i in 0..max_retries {
        match fs::read_dir(path) {
            Ok(dir) => return Ok(dir),
            Err(e) if i < max_retries - 1 => {
                log::debug!("pcid-spawner: waiting for {} (attempt {}/{}): {}", path, i + 1, max_retries, e);
                thread::sleep(Duration::from_millis(delay_ms));
            }
            Err(e) => return Err(e.into()),
        }
    }
    unreachable!()
}

fn main() -> Result<()> {
    let mut args = pico_args::Arguments::from_env();
    let config_path = args
        .free_from_str::<String>()
        .expect("failed to parse --config argument");

    common::setup_logging(
        "bus",
        "pci",
        "pci-spawner.log",
        common::output_level(),
        common::file_level(),
    );

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
    let config: Config = toml::from_str(&config_data)?;

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
