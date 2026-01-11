//! 9P client over virtio transport

use std::sync::Arc;
use std::sync::atomic::{AtomicU16, AtomicU32, Ordering};

use anyhow::{anyhow, Result};

use common::dma::Dma;
use virtio_core::spec::{Buffer, ChainBuilder, DescriptorFlags};
use virtio_core::transport::Queue;

use crate::protocol::*;

const MSIZE: u32 = 131072; // Maximum message size (128KB for good 9p performance)

/// Simple spin-polling for futures without an async runtime
fn spin_poll<F: std::future::Future>(mut future: F) -> F::Output {
    use std::pin::Pin;
    use std::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};

    // Create a no-op waker
    fn clone_fn(_: *const ()) -> RawWaker { RawWaker::new(std::ptr::null(), &VTABLE) }
    fn wake_fn(_: *const ()) {}
    fn wake_by_ref_fn(_: *const ()) {}
    fn drop_fn(_: *const ()) {}
    static VTABLE: RawWakerVTable = RawWakerVTable::new(clone_fn, wake_fn, wake_by_ref_fn, drop_fn);

    let waker = unsafe { Waker::from_raw(RawWaker::new(std::ptr::null(), &VTABLE)) };
    let mut cx = Context::from_waker(&waker);

    // SAFETY: We never move the future after pinning
    let mut future = unsafe { Pin::new_unchecked(&mut future) };

    loop {
        match future.as_mut().poll(&mut cx) {
            Poll::Ready(result) => return result,
            Poll::Pending => {
                // Spin and yield to let the device process
                for _ in 0..100 {
                    core::hint::spin_loop();
                }
            }
        }
    }
}
const VERSION: &str = "9P2000.L";

/// 9P client over virtio-9p
pub struct Client9p<'a> {
    queue: Arc<Queue<'a>>,
    tag_counter: AtomicU16,
    fid_counter: AtomicU32,
    root_fid: u32,
    msize: u32,
}

impl<'a> Client9p<'a> {
    pub fn new(queue: Arc<Queue<'a>>) -> Result<Self> {
        Ok(Self {
            queue,
            tag_counter: AtomicU16::new(1),
            fid_counter: AtomicU32::new(1),
            root_fid: 0,
            msize: MSIZE,
        })
    }

    fn next_tag(&self) -> u16 {
        self.tag_counter.fetch_add(1, Ordering::Relaxed)
    }

    pub fn alloc_fid(&self) -> u32 {
        self.fid_counter.fetch_add(1, Ordering::Relaxed)
    }

    /// Send a 9P message and receive response
    fn transact(&self, request: Vec<u8>) -> Result<Vec<u8>> {
        log::trace!("transact: sending {} bytes", request.len());

        // Allocate request buffer and copy data
        let mut req_dma = unsafe {
            Dma::<[u8]>::zeroed_slice(request.len())
                .map_err(|_| anyhow!("DMA alloc failed"))?
                .assume_init()
        };
        req_dma.copy_from_slice(&request);

        // Allocate response buffer
        let resp_dma = unsafe {
            Dma::<[u8]>::zeroed_slice(self.msize as usize)
                .map_err(|_| anyhow!("DMA alloc failed"))?
                .assume_init()
        };

        log::trace!("transact: DMA buffers allocated, building chain");

        let chain = ChainBuilder::new()
            .chain(Buffer::new_sized(&req_dma, req_dma.len()))
            .chain(Buffer::new_sized(&resp_dma, resp_dma.len()).flags(DescriptorFlags::WRITE_ONLY))
            .build();

        log::trace!("transact: calling queue.send()");
        // Use spin-polling instead of futures executor since we don't have an event loop
        let pending = self.queue.send(chain);
        let written = spin_poll(pending) as usize;
        log::trace!("transact: queue.send() returned {} bytes", written);

        // Parse response
        if written < Header::SIZE {
            return Err(anyhow!("response too short"));
        }

        let header = Header::decode(&resp_dma[..Header::SIZE])
            .ok_or_else(|| anyhow!("invalid response header"))?;

        let size = header.size as usize;
        if size > written || size > self.msize as usize {
            return Err(anyhow!("invalid response size"));
        }

        // Check for error response
        if header.typ == MsgType::Rerror as u8 {
            let mut parser = MessageParser::new(&resp_dma[Header::SIZE..size]);
            let errno = parser.get_u32().unwrap_or(0);
            return Err(anyhow!("9P error: errno={}", errno));
        }

        Ok(resp_dma[..size].to_vec())
    }

    /// Negotiate protocol version
    pub fn version(&self) -> Result<()> {
        let tag = self.next_tag();
        let msg = MessageBuilder::new(MsgType::Tversion, tag)
            .put_u32(self.msize)
            .put_str(VERSION)
            .finish();

        let resp = self.transact(msg)?;
        let mut parser = MessageParser::new(&resp);
        let header = parser.get_header().ok_or_else(|| anyhow!("no header"))?;

        if header.typ != MsgType::Rversion as u8 {
            return Err(anyhow!("unexpected response type: {}", header.typ));
        }

        let _msize = parser.get_u32().ok_or_else(|| anyhow!("no msize"))?;
        let version = parser.get_str().ok_or_else(|| anyhow!("no version"))?;

        if version != VERSION {
            return Err(anyhow!("version mismatch: got {}", version));
        }

        Ok(())
    }

    /// Attach to the filesystem root
    pub fn attach(&self, aname: &str) -> Result<Qid> {
        let tag = self.next_tag();
        let root_fid = 0u32; // Use fid 0 for root

        let msg = MessageBuilder::new(MsgType::Tattach, tag)
            .put_u32(root_fid)  // fid
            .put_u32(NOFID)     // afid (no auth)
            .put_str("")        // uname
            .put_str(aname)     // aname
            .put_u32(0)         // n_uname (9P2000.L extension)
            .finish();

        let resp = self.transact(msg)?;
        let mut parser = MessageParser::new(&resp);
        let header = parser.get_header().ok_or_else(|| anyhow!("no header"))?;

        if header.typ != MsgType::Rattach as u8 {
            return Err(anyhow!("attach failed: type={}", header.typ));
        }

        let qid = parser.get_qid().ok_or_else(|| anyhow!("no qid"))?;
        Ok(qid)
    }

    /// Walk from fid to path components, creating new_fid
    pub fn walk(&self, fid: u32, new_fid: u32, names: &[&str]) -> Result<Vec<Qid>> {
        let tag = self.next_tag();
        let mut builder = MessageBuilder::new(MsgType::Twalk, tag)
            .put_u32(fid)
            .put_u32(new_fid)
            .put_u16(names.len() as u16);
        for name in names {
            builder = builder.put_str(name);
        }
        let msg = builder.finish();

        let resp = self.transact(msg)?;
        let mut parser = MessageParser::new(&resp);
        let header = parser.get_header().ok_or_else(|| anyhow!("no header"))?;

        if header.typ != MsgType::Rwalk as u8 {
            return Err(anyhow!("walk failed: type={}", header.typ));
        }

        let nwqid = parser.get_u16().ok_or_else(|| anyhow!("no nwqid"))? as usize;
        let mut qids = Vec::with_capacity(nwqid);
        for _ in 0..nwqid {
            qids.push(parser.get_qid().ok_or_else(|| anyhow!("missing qid"))?);
        }

        Ok(qids)
    }

    /// Open a file (9P2000.L lopen)
    pub fn lopen(&self, fid: u32, flags: u32) -> Result<(Qid, u32)> {
        let tag = self.next_tag();
        let msg = MessageBuilder::new(MsgType::Tlopen, tag)
            .put_u32(fid)
            .put_u32(flags)
            .finish();

        let resp = self.transact(msg)?;
        let mut parser = MessageParser::new(&resp);
        let header = parser.get_header().ok_or_else(|| anyhow!("no header"))?;

        if header.typ != MsgType::Rlopen as u8 {
            return Err(anyhow!("lopen failed: type={}", header.typ));
        }

        let qid = parser.get_qid().ok_or_else(|| anyhow!("no qid"))?;
        let iounit = parser.get_u32().ok_or_else(|| anyhow!("no iounit"))?;

        Ok((qid, iounit))
    }

    /// Create a file (9P2000.L lcreate)
    pub fn lcreate(&self, fid: u32, name: &str, flags: u32, mode: u32, gid: u32) -> Result<(Qid, u32)> {
        let tag = self.next_tag();
        let msg = MessageBuilder::new(MsgType::Tlcreate, tag)
            .put_u32(fid)
            .put_str(name)
            .put_u32(flags)
            .put_u32(mode)
            .put_u32(gid)
            .finish();

        let resp = self.transact(msg)?;
        let mut parser = MessageParser::new(&resp);
        let header = parser.get_header().ok_or_else(|| anyhow!("no header"))?;

        if header.typ != MsgType::Rlcreate as u8 {
            return Err(anyhow!("lcreate failed: type={}", header.typ));
        }

        let qid = parser.get_qid().ok_or_else(|| anyhow!("no qid"))?;
        let iounit = parser.get_u32().ok_or_else(|| anyhow!("no iounit"))?;

        Ok((qid, iounit))
    }

    /// Read from file
    pub fn read(&self, fid: u32, offset: u64, count: u32) -> Result<Vec<u8>> {
        // Limit count to fit response in msize buffer
        // Response: header (7) + data_len (4) + data
        let max_data = self.msize.saturating_sub(7 + 4);
        let count = count.min(max_data);

        let tag = self.next_tag();
        let msg = MessageBuilder::new(MsgType::Tread, tag)
            .put_u32(fid)
            .put_u64(offset)
            .put_u32(count)
            .finish();

        let resp = self.transact(msg)?;
        let mut parser = MessageParser::new(&resp);
        let header = parser.get_header().ok_or_else(|| anyhow!("no header"))?;

        if header.typ != MsgType::Rread as u8 {
            return Err(anyhow!("read failed: type={}", header.typ));
        }

        let data = parser.get_data().ok_or_else(|| anyhow!("no data"))?;
        Ok(data.to_vec())
    }

    /// Write to file
    pub fn write(&self, fid: u32, offset: u64, data: &[u8]) -> Result<u32> {
        let tag = self.next_tag();
        let msg = MessageBuilder::new(MsgType::Twrite, tag)
            .put_u32(fid)
            .put_u64(offset)
            .put_data(data)
            .finish();

        let resp = self.transact(msg)?;
        let mut parser = MessageParser::new(&resp);
        let header = parser.get_header().ok_or_else(|| anyhow!("no header"))?;

        if header.typ != MsgType::Rwrite as u8 {
            return Err(anyhow!("write failed: type={}", header.typ));
        }

        let count = parser.get_u32().ok_or_else(|| anyhow!("no count"))?;
        Ok(count)
    }

    /// Get file attributes
    pub fn getattr(&self, fid: u32, mask: u64) -> Result<FileAttr> {
        let tag = self.next_tag();
        let msg = MessageBuilder::new(MsgType::Tgetattr, tag)
            .put_u32(fid)
            .put_u64(mask)
            .finish();

        let resp = self.transact(msg)?;
        let mut parser = MessageParser::new(&resp);
        let header = parser.get_header().ok_or_else(|| anyhow!("no header"))?;

        if header.typ != MsgType::Rgetattr as u8 {
            return Err(anyhow!("getattr failed: type={}", header.typ));
        }

        FileAttr::decode(&mut parser).ok_or_else(|| anyhow!("invalid attr"))
    }

    /// Set file attributes
    pub fn setattr(
        &self,
        fid: u32,
        valid: u32,
        mode: u32,
        uid: u32,
        gid: u32,
        size: u64,
        atime_sec: u64,
        atime_nsec: u64,
        mtime_sec: u64,
        mtime_nsec: u64,
    ) -> Result<()> {
        let tag = self.next_tag();
        let msg = MessageBuilder::new(MsgType::Tsetattr, tag)
            .put_u32(fid)
            .put_u32(valid)
            .put_u32(mode)
            .put_u32(uid)
            .put_u32(gid)
            .put_u64(size)
            .put_u64(atime_sec)
            .put_u64(atime_nsec)
            .put_u64(mtime_sec)
            .put_u64(mtime_nsec)
            .finish();

        let resp = self.transact(msg)?;
        let mut parser = MessageParser::new(&resp);
        let header = parser.get_header().ok_or_else(|| anyhow!("no header"))?;

        if header.typ != MsgType::Rsetattr as u8 {
            return Err(anyhow!("setattr failed: type={}", header.typ));
        }

        Ok(())
    }

    /// Read directory entries
    pub fn readdir(&self, fid: u32, offset: u64, count: u32) -> Result<Vec<DirEntry>> {
        let tag = self.next_tag();
        let msg = MessageBuilder::new(MsgType::Treaddir, tag)
            .put_u32(fid)
            .put_u64(offset)
            .put_u32(count)
            .finish();

        let resp = self.transact(msg)?;
        let mut parser = MessageParser::new(&resp);
        let header = parser.get_header().ok_or_else(|| anyhow!("no header"))?;

        if header.typ != MsgType::Rreaddir as u8 {
            return Err(anyhow!("readdir failed: type={}", header.typ));
        }

        let data = parser.get_data().ok_or_else(|| anyhow!("no data"))?;
        let mut entries = Vec::new();
        let mut entry_parser = MessageParser::new(data);

        while entry_parser.remaining().len() > 0 {
            if let Some(entry) = DirEntry::decode(&mut entry_parser) {
                entries.push(entry);
            } else {
                break;
            }
        }

        Ok(entries)
    }

    /// Get filesystem stats
    pub fn statfs(&self, fid: u32) -> Result<StatFs> {
        let tag = self.next_tag();
        let msg = MessageBuilder::new(MsgType::Tstatfs, tag)
            .put_u32(fid)
            .finish();

        let resp = self.transact(msg)?;
        let mut parser = MessageParser::new(&resp);
        let header = parser.get_header().ok_or_else(|| anyhow!("no header"))?;

        if header.typ != MsgType::Rstatfs as u8 {
            return Err(anyhow!("statfs failed: type={}", header.typ));
        }

        StatFs::decode(&mut parser).ok_or_else(|| anyhow!("invalid statfs"))
    }

    /// Close a fid
    pub fn clunk(&self, fid: u32) -> Result<()> {
        let tag = self.next_tag();
        let msg = MessageBuilder::new(MsgType::Tclunk, tag)
            .put_u32(fid)
            .finish();

        let resp = self.transact(msg)?;
        let mut parser = MessageParser::new(&resp);
        let header = parser.get_header().ok_or_else(|| anyhow!("no header"))?;

        if header.typ != MsgType::Rclunk as u8 {
            return Err(anyhow!("clunk failed: type={}", header.typ));
        }

        Ok(())
    }

    /// Remove a file
    pub fn unlinkat(&self, dirfid: u32, name: &str, flags: u32) -> Result<()> {
        let tag = self.next_tag();
        let msg = MessageBuilder::new(MsgType::Tunlinkat, tag)
            .put_u32(dirfid)
            .put_str(name)
            .put_u32(flags)
            .finish();

        let resp = self.transact(msg)?;
        let mut parser = MessageParser::new(&resp);
        let header = parser.get_header().ok_or_else(|| anyhow!("no header"))?;

        if header.typ != MsgType::Runlinkat as u8 {
            return Err(anyhow!("unlinkat failed: type={}", header.typ));
        }

        Ok(())
    }

    /// Create directory
    pub fn mkdir(&self, dirfid: u32, name: &str, mode: u32, gid: u32) -> Result<Qid> {
        let tag = self.next_tag();
        let msg = MessageBuilder::new(MsgType::Tmkdir, tag)
            .put_u32(dirfid)
            .put_str(name)
            .put_u32(mode)
            .put_u32(gid)
            .finish();

        let resp = self.transact(msg)?;
        let mut parser = MessageParser::new(&resp);
        let header = parser.get_header().ok_or_else(|| anyhow!("no header"))?;

        if header.typ != MsgType::Rmkdir as u8 {
            return Err(anyhow!("mkdir failed: type={}", header.typ));
        }

        parser.get_qid().ok_or_else(|| anyhow!("no qid"))
    }

    /// Sync file
    pub fn fsync(&self, fid: u32) -> Result<()> {
        let tag = self.next_tag();
        let msg = MessageBuilder::new(MsgType::Tfsync, tag)
            .put_u32(fid)
            .put_u32(0)  // datasync flag
            .finish();

        let resp = self.transact(msg)?;
        let mut parser = MessageParser::new(&resp);
        let header = parser.get_header().ok_or_else(|| anyhow!("no header"))?;

        if header.typ != MsgType::Rfsync as u8 {
            return Err(anyhow!("fsync failed: type={}", header.typ));
        }

        Ok(())
    }

    /// Rename a file (renameat)
    pub fn renameat(&self, olddirfid: u32, oldname: &str, newdirfid: u32, newname: &str) -> Result<()> {
        let tag = self.next_tag();
        let msg = MessageBuilder::new(MsgType::Trenameat, tag)
            .put_u32(olddirfid)
            .put_str(oldname)
            .put_u32(newdirfid)
            .put_str(newname)
            .finish();

        let resp = self.transact(msg)?;
        let mut parser = MessageParser::new(&resp);
        let header = parser.get_header().ok_or_else(|| anyhow!("no header"))?;

        if header.typ != MsgType::Rrenameat as u8 {
            return Err(anyhow!("renameat failed: type={}", header.typ));
        }

        Ok(())
    }

    /// Get the root fid (always 0 after attach)
    pub fn root_fid(&self) -> u32 {
        0
    }
}
