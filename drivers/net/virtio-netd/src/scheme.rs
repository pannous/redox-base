use std::sync::Arc;

use driver_network::NetworkAdapter;

use common::dma::Dma;

use virtio_core::spec::{Buffer, ChainBuilder, DescriptorFlags};
use virtio_core::transport::Queue;

use crate::{VirtHeader, MAX_BUFFER_LEN};

pub struct VirtioNet<'a> {
    mac_address: [u8; 6],

    /// Reciever Queue.
    rx: Arc<Queue<'a>>,
    rx_buffers: Vec<Dma<[u8]>>,

    /// Transmiter Queue.
    tx: Arc<Queue<'a>>,

    recv_head: u16,
}

impl<'a> VirtioNet<'a> {
    pub fn new(mac_address: [u8; 6], rx: Arc<Queue<'a>>, tx: Arc<Queue<'a>>) -> Result<Self, syscall::Error> {
        // Populate all of the `rx_queue` with buffers to maximize performence.
        let mut rx_buffers = vec![];
        for i in 0..(rx.descriptor_len() as usize) {
            let dma_buf = unsafe {
                match Dma::<[u8]>::zeroed_slice(MAX_BUFFER_LEN) {
                    Ok(buf) => buf.assume_init(),
                    Err(e) => {
                        log::error!("virtio-netd: failed to allocate RX buffer {}: {:?}", i, e);
                        return Err(e.into());
                    }
                }
            };
            rx_buffers.push(dma_buf);

            let chain = ChainBuilder::new()
                .chain(Buffer::new_unsized(&rx_buffers[i]).flags(DescriptorFlags::WRITE_ONLY))
                .build();

            // RX buffers are recycled via recycle_descriptor(), so we can ignore the future
            if rx.send(chain).is_none() {
                log::warn!("virtio-netd: failed to add RX buffer {} - no descriptors", i);
            }
        }

        Ok(Self {
            mac_address,

            rx,
            rx_buffers,
            tx,

            recv_head: 0,
        })
    }

    /// Returns the number of bytes read. Returns `0` if the operation would block.
    fn try_recv(&mut self, target: &mut [u8]) -> usize {
        let header_size = core::mem::size_of::<VirtHeader>();

        if self.recv_head == self.rx.used.head_index() {
            // The read would block.
            return 0;
        }

        let idx = self.rx.used.head_index() as usize;
        let element = self.rx.used.get_element_at(idx - 1);

        let descriptor_idx = element.table_index.get();
        let payload_size = element.written.get() as usize - header_size;

        // XXX: The header and packet are added as one output descriptor to the transmit queue,
        //      and the device is notified of the new entry (see 5.1.5 Device Initialization).
        let buffer = &self.rx_buffers[descriptor_idx as usize];
        // TODO: Check the header.
        let _header = unsafe { &*(buffer.as_ptr() as *const VirtHeader) };
        let packet = &buffer[header_size..(header_size + payload_size)];

        // Copy only as much as fits in the target buffer
        let copy_size = core::cmp::min(payload_size, target.len());
        target[..copy_size].copy_from_slice(&packet[..copy_size]);

        self.recv_head = self.rx.used.head_index();

        // Recycle the RX buffer back to the available ring for future packets
        log::info!("Recycling RX descriptor {} (recv_head now {})", descriptor_idx, self.recv_head);
        self.rx.recycle_descriptor(descriptor_idx as u16);

        copy_size
    }
}

impl<'a> NetworkAdapter for VirtioNet<'a> {
    fn mac_address(&mut self) -> [u8; 6] {
        self.mac_address
    }

    fn available_for_read(&mut self) -> usize {
        (self.rx.used.head_index() - self.recv_head).into()
    }

    fn read_packet(&mut self, buf: &mut [u8]) -> syscall::Result<Option<usize>> {
        let bytes = self.try_recv(buf);

        if bytes != 0 {
            // We read some bytes.
            Ok(Some(bytes))
        } else {
            Ok(None)
        }
    }

    fn write_packet(&mut self, buffer: &[u8]) -> syscall::Result<usize> {
        // Allocate DMA buffers for header and payload
        let header = match Dma::<VirtHeader>::zeroed() {
            Ok(h) => Box::leak(Box::new(unsafe { h.assume_init() })),
            Err(e) => {
                log::error!("virtio-netd: DMA header alloc failed: {:?}", e);
                return Err(e.into());
            }
        };

        let payload = match Dma::<[u8]>::zeroed_slice(buffer.len()) {
            Ok(p) => Box::leak(Box::new(unsafe { p.assume_init() })),
            Err(e) => {
                log::error!("virtio-netd: DMA payload alloc failed: {:?}", e);
                return Err(e.into());
            }
        };
        payload.copy_from_slice(buffer);

        let chain = ChainBuilder::new()
            .chain(Buffer::new(header))
            .chain(Buffer::new_unsized(payload))
            .build();

        // send() now reclaims completed TX descriptors automatically before checking availability
        match self.tx.send(chain) {
            Some(_) => Ok(buffer.len()),
            None => {
                // No descriptors available even after reclaiming - would block
                log::warn!("virtio-netd: TX queue full, dropping packet ({} bytes)", buffer.len());
                Err(syscall::Error::new(syscall::EWOULDBLOCK))
            }
        }
    }
}
