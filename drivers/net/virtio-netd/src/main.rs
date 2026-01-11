mod scheme;

use std::fs::File;
use std::io::{Read, Write};
use std::mem;

use driver_network::NetworkScheme;
use pcid_interface::PciFunctionHandle;

use scheme::VirtioNet;

pub const VIRTIO_NET_F_MAC: u32 = 5;

#[derive(Debug)]
#[repr(C)]
pub struct VirtHeader {
    pub flags: u8,
    pub gso_type: u8,
    pub hdr_len: u16,
    pub gso_size: u16,
    pub csum_start: u16,
    pub csum_offset: u16,
    pub num_buffers: u16,
}

static_assertions::const_assert_eq!(core::mem::size_of::<VirtHeader>(), 12);

const MAX_BUFFER_LEN: usize = 65535;
fn main() {
    pcid_interface::pci_daemon(daemon_runner);
}

fn daemon_runner(daemon: daemon::Daemon, pcid_handle: PciFunctionHandle) -> ! {
    if let Err(e) = deamon(daemon, pcid_handle) {
        log::error!("virtio-netd: daemon failed: {}", e);
        std::process::exit(1);
    }
    unreachable!();
}

fn deamon(
    daemon: daemon::Daemon,
    mut pcid_handle: PciFunctionHandle,
) -> Result<(), Box<dyn std::error::Error>> {
    common::setup_logging(
        "net",
        "pci",
        "virtio-netd",
        common::output_level(),
        common::file_level(),
    );

    // Double check that we have the right device.
    //
    // 0x1000 - virtio-net
    let pci_config = pcid_handle.config();

    if pci_config.func.full_device_id.device_id != 0x1000 {
        return Err(format!(
            "virtio-netd: unexpected device ID 0x{:04X}, expected 0x1000",
            pci_config.func.full_device_id.device_id
        ).into());
    }
    log::debug!("virtio-net: initiating startup sequence");

    let device = virtio_core::probe_device(&mut pcid_handle)?;
    let device_space = device.device_space;

    // Negotiate device features:
    let mac_address = if device.transport.check_device_feature(VIRTIO_NET_F_MAC) {
        let mac = unsafe {
            [
                core::ptr::read_volatile(device_space.add(0)),
                core::ptr::read_volatile(device_space.add(1)),
                core::ptr::read_volatile(device_space.add(2)),
                core::ptr::read_volatile(device_space.add(3)),
                core::ptr::read_volatile(device_space.add(4)),
                core::ptr::read_volatile(device_space.add(5)),
            ]
        };

        log::debug!(
            "virtio-net: device MAC is {:>02X}:{:>02X}:{:>02X}:{:>02X}:{:>02X}:{:>02X}",
            mac[0],
            mac[1],
            mac[2],
            mac[3],
            mac[4],
            mac[5]
        );

        device.transport.ack_driver_feature(VIRTIO_NET_F_MAC);
        mac
    } else {
        log::warn!("virtio-net: device does not support MAC feature, using default");
        [0x52, 0x54, 0x00, 0x12, 0x34, 0x56] // Default QEMU MAC
    };

    device.transport.finalize_features();

    // Allocate the recieve and transmit queues:
    //
    // > Empty buffers are placed in one virtqueue for receiving
    // > packets, and outgoing packets are enqueued into another
    // > for transmission in that order.
    //
    // TODO(andypython): Should we use the same IRQ vector for both?
    let rx_queue = device
        .transport
        .setup_queue(virtio_core::MSIX_PRIMARY_VECTOR, &device.irq_handle)?;

    let tx_queue = device
        .transport
        .setup_queue(virtio_core::MSIX_PRIMARY_VECTOR, &device.irq_handle)?;

    device.transport.run_device();

    let mut name = pci_config.func.name();
    name.push_str("_virtio_net");

    let device = match VirtioNet::new(mac_address, rx_queue, tx_queue) {
        Ok(dev) => dev,
        Err(e) => {
            log::error!("virtio-netd: failed to initialize device: {:?}", e);
            return Err(format!("device init failed: {:?}", e).into());
        }
    };
    let mut scheme = NetworkScheme::new(
        move || {
            //TODO: do device init in this function to prevent hangs
            device
        },
        daemon,
        format!("network.{name}"),
    );

    let mut event_queue = File::open("/scheme/event")?;
    event_queue.write(&syscall::Event {
        id: scheme.event_handle().raw(),
        flags: syscall::EVENT_READ,
        data: 0,
    })?;

    if let Err(e) = libredox::call::setrens(0, 0) {
        log::warn!("virtio-netd: failed to enter null namespace: {:?}", e);
    }

    scheme.tick()?;

    loop {
        event_queue.read(&mut [0; mem::size_of::<syscall::Event>()])?; // Wait for event
        scheme.tick()?;
    }
}
