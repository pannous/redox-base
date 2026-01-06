#![deny(trivial_numeric_casts, unused_allocation)]
#![allow(unsafe_op_in_unsafe_fn)]

use std::sync::Arc;

use anyhow::{anyhow, Context, Result};
use thiserror::Error;

use pcid_interface::*;
use virtio_core::spec::*;
use virtio_core::transport::Transport;

mod protocol;
mod scheme;
mod client;

use client::Client9p;
use scheme::Scheme9p;

#[derive(Debug, Error)]
pub enum Error {
    #[error("capability {0:?} not found")]
    InCapable(CfgType),
    #[error("failed to map memory")]
    Physmap,
    #[error("failed to allocate an interrupt vector")]
    ExhaustedInt,
    #[error("9P protocol error: {0}")]
    Protocol(String),
    #[error("syscall error")]
    SyscallError(syscall::Error),
}

fn main() {
    pcid_interface::pci_daemon(daemon_runner);
}

fn daemon_runner(redox_daemon: daemon::Daemon, pcid_handle: PciFunctionHandle) -> ! {
    daemon(redox_daemon, pcid_handle).unwrap();
    unreachable!();
}

fn daemon(daemon: daemon::Daemon, mut pcid_handle: PciFunctionHandle) -> Result<()> {
    common::setup_logging(
        "fs",
        "pci",
        "virtio-9pd",
        common::output_level(),
        common::file_level(),
    );

    let pci_config = pcid_handle.config();

    // virtio-9p has device ID 0x1009 (legacy) or 0x1049 (modern)
    let device_id = pci_config.func.full_device_id.device_id;
    if device_id != 0x1009 && device_id != 0x1049 {
        log::error!("virtio-9pd: unexpected device ID: {:#x}", device_id);
        return Err(anyhow!("unexpected device ID"));
    }

    log::info!("virtio-9pd: initiating startup sequence");

    let device = virtio_core::probe_device(&mut pcid_handle)?;

    // Read the mount tag from device config
    let mount_tag = read_mount_tag(&device.transport);
    log::info!("virtio-9pd: mount tag = {:?}", mount_tag);

    device.transport.finalize_features();

    // Set up the single virtqueue for 9P
    let queue = device
        .transport
        .setup_queue(virtio_core::MSIX_PRIMARY_VECTOR, &device.irq_handle)?;

    device.transport.run_device();

    log::info!("virtio-9pd: device initialized");

    // Create 9P client
    let client = Client9p::new(queue)?;

    // Negotiate version
    client.version()?;
    log::info!("virtio-9pd: version negotiated");

    // Attach to root
    log::info!("virtio-9pd: calling attach...");
    let root_qid = client.attach(&mount_tag)?;
    log::info!("virtio-9pd: attached to root, qid={:?}", root_qid);

    // Create scheme name based on mount tag
    let scheme_name = if mount_tag.is_empty() {
        format!("9p.{}", pci_config.func.name())
    } else {
        format!("9p.{}", mount_tag)
    };

    log::info!("virtio-9pd: creating scheme '{}'", scheme_name);

    let socket = redox_scheme::Socket::create(&scheme_name)
        .context("failed to create scheme socket")?;

    let mut scheme = Scheme9p::new(scheme_name, client, root_qid);

    libredox::call::setrens(0, 0).expect("virtio-9pd: failed to enter null namespace");

    daemon.ready();

    log::info!("virtio-9pd: ready, serving requests");

    loop {
        let Some(request) = socket
            .next_request(redox_scheme::SignalBehavior::Restart)
            .context("failed to get next request")?
        else {
            break;
        };

        match request.kind() {
            redox_scheme::RequestKind::Call(call) => {
                let response = call.handle_sync(&mut scheme);
                socket
                    .write_response(response, redox_scheme::SignalBehavior::Restart)
                    .context("failed to write response")?;
            }
            redox_scheme::RequestKind::OnClose { id } => {
                scheme.on_close(id);
            }
            _ => (),
        }
    }

    Ok(())
}

/// Read the mount tag from virtio-9p device config space
fn read_mount_tag(transport: &Arc<dyn Transport>) -> String {
    // Device config layout:
    // u16 tag_len
    // u8[tag_len] tag
    let tag_len = transport.load_config(0, 2) as usize;
    if tag_len == 0 || tag_len > 256 {
        return String::new();
    }

    let mut tag_bytes = Vec::with_capacity(tag_len);
    for i in 0..tag_len {
        let byte = transport.load_config(2 + i as u8, 1) as u8;
        if byte == 0 {
            break;
        }
        tag_bytes.push(byte);
    }

    String::from_utf8(tag_bytes).unwrap_or_default()
}
