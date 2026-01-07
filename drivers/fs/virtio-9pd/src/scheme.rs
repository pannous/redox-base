//! Redox scheme implementation for 9P filesystem

use std::collections::BTreeMap;

use syscall::dirent::{DirEntry, DirentBuf, DirentKind};
use syscall::error::{EBADF, EBADFD, EIO, EISDIR, ENOENT, ENOSYS, ENOTDIR};
use syscall::flag::{O_ACCMODE, O_CREAT, O_DIRECTORY, O_RDONLY, O_RDWR, O_STAT, O_TRUNC, O_WRONLY};
use syscall::schemev2::NewFdFlags;
use syscall::{Error, EventFlags, Result, Stat, StatVfs, TimeSpec};

use redox_scheme::scheme::SchemeSync;
use redox_scheme::{CallerCtx, OpenResult};

use crate::client::Client9p;
use crate::protocol::{self, FileAttr, P9_GETATTR_BASIC, Qid};

/// State for an open file handle
struct Handle {
    /// 9P fid for this handle
    fid: u32,
    /// Path used to open (for fpath)
    path: String,
    /// QID from open/walk
    qid: Qid,
    /// Open flags
    flags: usize,
    /// Current directory read offset (for readdir)
    dir_offset: u64,
}

/// Redox scheme for 9P filesystem
pub struct Scheme9p<'a> {
    scheme_name: String,
    client: Client9p<'a>,
    root_qid: Qid,
    /// Map from Redox fd number to Handle
    handles: BTreeMap<usize, Handle>,
    /// Next handle ID
    next_handle: usize,
}

impl<'a> Scheme9p<'a> {
    pub fn new(scheme_name: String, client: Client9p<'a>, root_qid: Qid) -> Self {
        Self {
            scheme_name,
            client,
            root_qid,
            handles: BTreeMap::new(),
            next_handle: 1,
        }
    }

    /// Walk a path from root, returning the final QID
    fn walk_path(&self, path: &str) -> Result<(u32, Qid)> {
        let new_fid = self.client.alloc_fid();

        // Split path into components
        let components: Vec<&str> = path
            .split('/')
            .filter(|s| !s.is_empty())
            .collect();

        let qids = self.client
            .walk(self.client.root_fid(), new_fid, &components)
            .map_err(|e| {
                log::debug!("walk failed for '{}': {}", path, e);
                Error::new(ENOENT)
            })?;

        // If we got fewer QIDs than path components, the walk failed partway
        if components.len() > 0 && qids.len() != components.len() {
            let _ = self.client.clunk(new_fid);
            return Err(Error::new(ENOENT));
        }

        // Return the final QID (or root if path was empty)
        let qid = qids.last().copied().unwrap_or(self.root_qid);
        Ok((new_fid, qid))
    }

    /// Convert 9P FileAttr to Redox Stat
    fn attr_to_stat(&self, attr: &FileAttr) -> Stat {
        Stat {
            st_dev: 0,
            st_ino: attr.qid.path,
            st_mode: attr.mode as u16,
            st_nlink: attr.nlink as u32,
            st_uid: attr.uid,
            st_gid: attr.gid,
            st_size: attr.size,
            st_blksize: attr.blksize as u32,
            st_blocks: attr.blocks,
            st_atime: attr.atime_sec,
            st_atime_nsec: attr.atime_nsec as u32,
            st_mtime: attr.mtime_sec,
            st_mtime_nsec: attr.mtime_nsec as u32,
            st_ctime: attr.ctime_sec,
            st_ctime_nsec: attr.ctime_nsec as u32,
        }
    }

    /// Convert Redox open flags to 9P open flags
    fn to_9p_flags(&self, flags: usize) -> u32 {
        let mut p9_flags = match flags & O_ACCMODE {
            O_RDONLY => protocol::P9_RDONLY,
            O_WRONLY => protocol::P9_WRONLY,
            O_RDWR => protocol::P9_RDWR,
            _ => protocol::P9_RDONLY,
        };

        if flags & O_TRUNC != 0 {
            p9_flags |= protocol::P9_TRUNC;
        }
        if flags & O_CREAT != 0 {
            p9_flags |= protocol::P9_CREATE;
        }

        p9_flags
    }

    pub fn on_close(&mut self, id: usize) {
        if let Some(handle) = self.handles.remove(&id) {
            let _ = self.client.clunk(handle.fid);
        }
    }
}

impl SchemeSync for Scheme9p<'_> {
    fn open(&mut self, path: &str, flags: usize, ctx: &CallerCtx) -> Result<OpenResult> {
        log::trace!("open: path='{}' flags={:#x}", path, flags);

        // Walk to the path
        let (fid, qid) = match self.walk_path(path) {
            Ok(r) => r,
            Err(e) if flags & O_CREAT != 0 => {
                // File doesn't exist but O_CREAT is set - try to create it
                // First walk to parent directory
                let (parent_path, name) = match path.rfind('/') {
                    Some(i) => (&path[..i], &path[i + 1..]),
                    None => ("", path),
                };

                let (parent_fid, _parent_qid) = if parent_path.is_empty() {
                    // Clone root fid
                    let new_fid = self.client.alloc_fid();
                    self.client
                        .walk(self.client.root_fid(), new_fid, &[])
                        .map_err(|_| Error::new(EIO))?;
                    (new_fid, self.root_qid)
                } else {
                    self.walk_path(parent_path)?
                };

                // Create the file
                let mode = (flags & 0o7777) as u32 | 0o100000; // S_IFREG
                let p9_flags = self.to_9p_flags(flags);

                let (qid, _iounit) = self.client
                    .lcreate(parent_fid, name, p9_flags, mode, ctx.gid)
                    .map_err(|e| {
                        log::debug!("lcreate failed: {}", e);
                        Error::new(EIO)
                    })?;

                (parent_fid, qid)
            }
            Err(e) => return Err(e),
        };

        // Check directory flag consistency
        // Allow O_DIRECTORY on regular files if O_STAT is set (for stat operations)
        let is_dir = qid.is_dir();
        if flags & O_DIRECTORY != 0 && !is_dir && flags & O_STAT == 0 {
            let _ = self.client.clunk(fid);
            return Err(Error::new(ENOTDIR));
        }
        if flags & O_STAT == 0 && flags & O_DIRECTORY == 0 && is_dir {
            let _ = self.client.clunk(fid);
            return Err(Error::new(EISDIR));
        }

        // Open the file (unless O_STAT)
        if flags & O_STAT == 0 {
            let p9_flags = self.to_9p_flags(flags);
            let _ = self.client.lopen(fid, p9_flags).map_err(|e| {
                log::debug!("lopen failed: {}", e);
                let _ = self.client.clunk(fid);
                Error::new(EIO)
            })?;
        }

        // Allocate handle
        let handle_id = self.next_handle;
        self.next_handle += 1;

        self.handles.insert(handle_id, Handle {
            fid,
            path: path.to_string(),
            qid,
            flags,
            dir_offset: 0,
        });

        Ok(OpenResult::ThisScheme {
            number: handle_id,
            flags: NewFdFlags::POSITIONED,
        })
    }

    fn read(
        &mut self,
        id: usize,
        buf: &mut [u8],
        offset: u64,
        fcntl_flags: u32,
        _ctx: &CallerCtx,
    ) -> Result<usize> {
        let handle = self.handles.get(&id).ok_or(Error::new(EBADFD))?;

        if handle.qid.is_dir() {
            return Err(Error::new(EISDIR));
        }

        if !matches!((fcntl_flags as usize) & O_ACCMODE, O_RDONLY | O_RDWR) {
            return Err(Error::new(EBADF));
        }

        let data = self.client
            .read(handle.fid, offset, buf.len() as u32)
            .map_err(|e| {
                log::debug!("read failed: {}", e);
                Error::new(EIO)
            })?;

        let len = data.len().min(buf.len());
        buf[..len].copy_from_slice(&data[..len]);
        Ok(len)
    }

    fn write(
        &mut self,
        id: usize,
        buf: &[u8],
        offset: u64,
        _fcntl_flags: u32,
        _ctx: &CallerCtx,
    ) -> Result<usize> {
        let handle = self.handles.get(&id).ok_or(Error::new(EBADFD))?;

        if handle.qid.is_dir() {
            return Err(Error::new(EISDIR));
        }

        let count = self.client
            .write(handle.fid, offset, buf)
            .map_err(|e| {
                log::debug!("write failed: {}", e);
                Error::new(EIO)
            })?;

        Ok(count as usize)
    }

    fn getdents<'buf>(
        &mut self,
        id: usize,
        mut buf: DirentBuf<&'buf mut [u8]>,
        opaque_offset: u64,
    ) -> Result<DirentBuf<&'buf mut [u8]>> {
        let handle = self.handles.get_mut(&id).ok_or(Error::new(EBADFD))?;

        if !handle.qid.is_dir() {
            return Err(Error::new(ENOTDIR));
        }

        // Read directory entries from 9P
        let entries = self.client
            .readdir(handle.fid, opaque_offset, 4096)
            .map_err(|e| {
                log::debug!("readdir failed: {}", e);
                Error::new(EIO)
            })?;

        for entry in entries {
            let kind = if entry.qid.is_dir() {
                DirentKind::Directory
            } else {
                DirentKind::Regular
            };

            buf.entry(DirEntry {
                inode: entry.qid.path,
                name: &entry.name,
                kind,
                next_opaque_id: entry.offset,
            })?;
        }

        Ok(buf)
    }

    fn fstat(&mut self, id: usize, stat: &mut Stat, _ctx: &CallerCtx) -> Result<()> {
        let handle = self.handles.get(&id).ok_or(Error::new(EBADFD))?;

        let attr = self.client
            .getattr(handle.fid, P9_GETATTR_BASIC)
            .map_err(|e| {
                log::debug!("getattr failed: {}", e);
                Error::new(EIO)
            })?;

        *stat = self.attr_to_stat(&attr);
        Ok(())
    }

    fn fstatvfs(&mut self, id: usize, stat: &mut StatVfs, _ctx: &CallerCtx) -> Result<()> {
        let handle = self.handles.get(&id).ok_or(Error::new(EBADFD))?;

        let fs = self.client
            .statfs(handle.fid)
            .map_err(|e| {
                log::debug!("statfs failed: {}", e);
                Error::new(EIO)
            })?;

        *stat = StatVfs {
            f_bsize: fs.bsize,
            f_blocks: fs.blocks,
            f_bfree: fs.bfree,
            f_bavail: fs.bavail,
        };

        Ok(())
    }

    fn fpath(&mut self, id: usize, buf: &mut [u8], _ctx: &CallerCtx) -> Result<usize> {
        let handle = self.handles.get(&id).ok_or(Error::new(EBADFD))?;

        let path = format!("/{}/{}", self.scheme_name, handle.path);
        let bytes = path.as_bytes();
        let len = bytes.len().min(buf.len());
        buf[..len].copy_from_slice(&bytes[..len]);
        Ok(len)
    }

    fn fsync(&mut self, id: usize, _ctx: &CallerCtx) -> Result<()> {
        let handle = self.handles.get(&id).ok_or(Error::new(EBADFD))?;

        self.client.fsync(handle.fid).map_err(|e| {
            log::debug!("fsync failed: {}", e);
            Error::new(EIO)
        })
    }

    fn unlinkat(&mut self, id: usize, path: &str, flags: usize, _ctx: &CallerCtx) -> Result<()> {
        let handle = self.handles.get(&id).ok_or(Error::new(EBADFD))?;

        // AT_REMOVEDIR flag
        let rmdir = flags & syscall::AT_REMOVEDIR != 0;
        let p9_flags = if rmdir { 0x200 } else { 0 }; // AT_REMOVEDIR in 9P

        self.client
            .unlinkat(handle.fid, path, p9_flags)
            .map_err(|e| {
                log::debug!("unlinkat failed: {}", e);
                Error::new(EIO)
            })
    }

    fn fcntl(&mut self, _id: usize, _cmd: usize, _arg: usize, _ctx: &CallerCtx) -> Result<usize> {
        Ok(0)
    }

    fn fevent(&mut self, _id: usize, _flags: EventFlags, _ctx: &CallerCtx) -> Result<EventFlags> {
        Err(Error::new(ENOSYS))
    }

    fn fchmod(&mut self, _id: usize, _mode: u16, _ctx: &CallerCtx) -> Result<()> {
        // TODO: implement setattr
        Ok(())
    }

    fn fchown(&mut self, _id: usize, _uid: u32, _gid: u32, _ctx: &CallerCtx) -> Result<()> {
        // TODO: implement setattr
        Ok(())
    }

    fn ftruncate(&mut self, _id: usize, _len: u64, _ctx: &CallerCtx) -> Result<()> {
        // TODO: implement setattr
        Err(Error::new(ENOSYS))
    }

    fn futimens(&mut self, _id: usize, _times: &[TimeSpec], _ctx: &CallerCtx) -> Result<()> {
        // TODO: implement setattr
        Ok(())
    }

    fn frename(&mut self, _id: usize, _path: &str, _ctx: &CallerCtx) -> Result<usize> {
        Err(Error::new(ENOSYS))
    }

    fn mmap_prep(
        &mut self,
        _id: usize,
        _offset: u64,
        _size: usize,
        _flags: syscall::MapFlags,
        _ctx: &CallerCtx,
    ) -> Result<usize> {
        Err(Error::new(ENOSYS))
    }
}
