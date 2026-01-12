mod scheme;

use std::io::{Read, Write};
use std::os::unix::io::AsRawFd;

use driver_network::NetworkScheme;
use event::{user_data, EventFlags, UserData};
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
    let mut irq_handle = device.irq_handle;
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
    // Use setup_queue_no_irq to avoid spawning IRQ threads - we handle IRQs
    // in our main event loop instead for more responsive packet handling.
    let rx_queue = device
        .transport
        .setup_queue_no_irq(virtio_core::MSIX_PRIMARY_VECTOR)?;

    let tx_queue = device
        .transport
        .setup_queue_no_irq(virtio_core::MSIX_PRIMARY_VECTOR)?;

    device.transport.run_device();

    let mut name = pci_config.func.name();
    name.push_str("_virtio_net");

    let dev = match VirtioNet::new(mac_address, rx_queue, tx_queue) {
        Ok(dev) => dev,
        Err(e) => {
            log::error!("virtio-netd: failed to initialize device: {:?}", e);
            return Err(format!("device init failed: {:?}", e).into());
        }
    };
    let mut scheme = NetworkScheme::new(
        move || dev,
        daemon,
        format!("network.{name}"),
    );

    user_data! {
        enum Source {
            Irq,
            Scheme,
        }
    }

    let irq_fd = irq_handle.as_raw_fd();
    eprintln!("DEBUG: virtio-netd: IRQ fd = {}", irq_fd);

    // Create event queue using raw API for timeout support
    let queue_fd = unsafe { event::raw::redox_event_queue_create_v1(0) };
    if queue_fd == !0 {
        return Err("virtio-netd: failed to create event queue".into());
    }

    // Subscribe to IRQ events
    let result = unsafe {
        event::raw::redox_event_queue_ctl_v1(
            queue_fd,
            irq_fd as usize,
            EventFlags::READ.bits(),
            Source::Irq.into_user_data(),
        )
    };
    if result == !0 {
        return Err("virtio-netd: failed to subscribe to IRQ events".into());
    }

    // Subscribe to scheme events
    let result = unsafe {
        event::raw::redox_event_queue_ctl_v1(
            queue_fd,
            scheme.event_handle().raw(),
            EventFlags::READ.bits(),
            Source::Scheme.into_user_data(),
        )
    };
    if result == !0 {
        return Err("virtio-netd: failed to subscribe to scheme events".into());
    }

    if let Err(e) = libredox::call::setrens(0, 0) {
        log::warn!("virtio-netd: failed to enter null namespace: {:?}", e);
    }

    scheme.tick()?;

    eprintln!("DEBUG: virtio-netd: entering polling event loop");

    let mut event_buf = [event::raw::RawEventV1::default()];
    let mut poll_count: u64 = 0;

    // Simple polling loop: check for events, then sleep briefly
    loop {
        // Non-blocking check for events
        // We can't use timeout on event queue, so we poll in a tight loop
        // with short sleeps between iterations

        loop {
            // Try to get an event (this might block if nothing is ready)
            let count = unsafe {
                event::raw::redox_event_queue_get_events_v1(
                    queue_fd,
                    event_buf.as_mut_ptr(),
                    1,
                    0,
                    core::ptr::null(),
                    core::ptr::null(),
                )
            };

            if count == 0 || count == !0 {
                // No event, break to poll the device
                break;
            }

            let event = &event_buf[0];
            let user_data = event.user_data;

            if user_data == Source::Irq.into_user_data() {
                eprintln!("DEBUG: virtio-netd: IRQ event");
                let mut irq = [0u8; 8];
                let _ = irq_handle.read(&mut irq);
                let _ = irq_handle.write(&irq);
            }
            // For any event, tick the scheme
            scheme.tick()?;
        }

        // Poll the device even without events (for packet reception)
        poll_count += 1;
        if poll_count % 1000 == 1 {
            eprintln!("DEBUG: virtio-netd poll #{}", poll_count);
        }
        scheme.tick()?;

        // Yield to other threads instead of sleeping
        std::thread::yield_now();
    }
}
