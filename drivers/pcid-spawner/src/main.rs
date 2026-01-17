use std::fs;
use std::process::{Child, Command};

use anyhow::{anyhow, Context, Result};

use pcid_interface::config::Config;
use pcid_interface::PciFunctionHandle;

// Track spawned drivers for parallel loading
struct SpawnedDriver {
    name: String,
    child: Child,
    channel_fd: i32,
}

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
    // Also wait for directory to have at least one entry
    for i in 0..100 {
        match fs::read_dir(path) {
            Ok(dir) => {
                // Peek to see if there are any entries
                let entries: Vec<_> = dir.collect();
                let count = entries.len();
                if count > 0 {
                    eprintln!("pcid-spawner: found {} with {} devices after {} attempts", path, count, i + 1);
                    // Return a new iterator since we consumed the original
                    return fs::read_dir(path).map_err(Into::into);
                }
                eprintln!("pcid-spawner: {} exists but empty, retrying (attempt {})", path, i + 1);
                busy_wait_ms(300);
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
    eprintln!("pcid-spawner: starting [BUILD-2026-01-17-A]");

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
    let dir_iter = wait_for_scheme("/scheme/pci", 50, 100)?;
    eprintln!("pcid-spawner: starting device enumeration (parallel mode)");

    // Collect spawned drivers for parallel execution
    let mut spawned_drivers: Vec<SpawnedDriver> = Vec::new();

    for entry in dir_iter {
        let entry = entry.context("failed to get entry")?;
        let device_path = entry.path();
        log::trace!("ENTRY: {}", device_path.to_string_lossy());

        eprintln!("pcid-spawner: trying {}", device_path.display());
        let mut handle = match PciFunctionHandle::connect_by_path(&device_path) {
            Ok(handle) => handle,
            Err(err) => {
                // Either the device is gone or it is already in-use by a driver.
                eprintln!(
                    "pcid-spawner: {} already in use: {err}",
                    device_path.display(),
                );
                continue;
            }
        };

        let full_device_id = handle.config().func.full_device_id;

        eprintln!(
            "pcid-spawner: PCI {} vendor={:04x} device={:04x} class={:02x}",
            handle.config().func.addr,
            full_device_id.vendor_id,
            full_device_id.device_id,
            full_device_id.class
        );

        let Some(driver) = config
            .drivers
            .iter()
            .find(|driver| driver.match_function(&full_device_id))
        else {
            eprintln!("pcid-spawner: no driver for {:04x}:{:04x}", full_device_id.vendor_id, full_device_id.device_id);
            continue;
        };
        let driver_name = driver.name.clone();
        eprintln!("pcid-spawner: MATCHED {:04x} -> {:?}", full_device_id.device_id, driver_name);

        let mut args = driver.command.iter();

        let program = args
            .next()
            .ok_or_else(|| anyhow!("driver configuration entry did not have any command!"))?;
        let program = if program.starts_with('/') {
            program.to_owned()
        } else {
            "/usr/lib/drivers/".to_owned() + program
        };

        let mut command = Command::new(&program);
        command.args(args);

        log::debug!("pcid-spawner: spawn {:?}", command);

        handle.enable_device();

        let channel_fd = handle.into_inner_fd();
        command.env("PCID_CLIENT_CHANNEL", channel_fd.to_string());
        // Suppress INFO/DEBUG logging for drivers (change to "info" or "debug" for verbose)
        command.env("RUST_LOG", "warn");

        // Spawn driver in parallel instead of blocking
        match command.spawn() {
            Ok(child) => {
                eprintln!("pcid-spawner: spawned {} (pid unknown)", driver_name);
                spawned_drivers.push(SpawnedDriver {
                    name: driver_name,
                    child,
                    channel_fd,
                });
            }
            Err(err) => {
                log::error!("pcid-spawner: failed to spawn {}: {err}", driver_name);
                syscall::close(channel_fd as usize).unwrap();
            }
        }
    }

    // Wait for all spawned drivers to complete
    eprintln!("pcid-spawner: waiting for {} drivers to initialize", spawned_drivers.len());
    for mut spawned in spawned_drivers {
        match spawned.child.wait() {
            Ok(status) if !status.success() => {
                log::error!("pcid-spawner: driver {} failed with {}", spawned.name, status);
            }
            Ok(_) => {
                eprintln!("pcid-spawner: driver {} completed", spawned.name);
            }
            Err(err) => {
                log::error!("pcid-spawner: failed to wait for {}: {err}", spawned.name);
            }
        }
        syscall::close(spawned.channel_fd as usize).unwrap();
    }

    eprintln!("pcid-spawner: all drivers initialized");
    Ok(())
}
