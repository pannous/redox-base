//! 9P2000.L protocol implementation
//!
//! This is a minimal implementation supporting the operations needed for
//! a read-only or read-write filesystem mount via virtio-9p.


// 9P2000.L message types
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MsgType {
    // Version
    Tversion = 100,
    Rversion = 101,
    // Auth (not used with virtio)
    Tauth = 102,
    Rauth = 103,
    // Attach
    Tattach = 104,
    Rattach = 105,
    // Error
    Rerror = 107,
    // Flush
    Tflush = 108,
    Rflush = 109,
    // Walk
    Twalk = 110,
    Rwalk = 111,
    // Open (9P2000)
    Topen = 112,
    Ropen = 113,
    // Create (9P2000)
    Tcreate = 114,
    Rcreate = 115,
    // Read
    Tread = 116,
    Rread = 117,
    // Write
    Twrite = 118,
    Rwrite = 119,
    // Clunk
    Tclunk = 120,
    Rclunk = 121,
    // Remove
    Tremove = 122,
    Rremove = 123,
    // Stat (9P2000)
    Tstat = 124,
    Rstat = 125,
    // Wstat (9P2000)
    Twstat = 126,
    Rwstat = 127,
    // 9P2000.L extensions
    Tstatfs = 8,
    Rstatfs = 9,
    Tlopen = 12,
    Rlopen = 13,
    Tlcreate = 14,
    Rlcreate = 15,
    Tsymlink = 16,
    Rsymlink = 17,
    Tmknod = 18,
    Rmknod = 19,
    Trename = 20,
    Rrename = 21,
    Treadlink = 22,
    Rreadlink = 23,
    Tgetattr = 24,
    Rgetattr = 25,
    Tsetattr = 26,
    Rsetattr = 27,
    Txattrwalk = 30,
    Rxattrwalk = 31,
    Txattrcreate = 32,
    Rxattrcreate = 33,
    Treaddir = 40,
    Rreaddir = 41,
    Tfsync = 50,
    Rfsync = 51,
    Tlock = 52,
    Rlock = 53,
    Tgetlock = 54,
    Rgetlock = 55,
    Tlink = 70,
    Rlink = 71,
    Tmkdir = 72,
    Rmkdir = 73,
    Trenameat = 74,
    Rrenameat = 75,
    Tunlinkat = 76,
    Runlinkat = 77,
}

// QID type flags
pub const QID_DIR: u8 = 0x80;
pub const QID_APPEND: u8 = 0x40;
pub const QID_EXCL: u8 = 0x20;
pub const QID_MOUNT: u8 = 0x10;
pub const QID_AUTH: u8 = 0x08;
pub const QID_TMP: u8 = 0x04;
pub const QID_SYMLINK: u8 = 0x02;
pub const QID_FILE: u8 = 0x00;

// Getattr request mask bits
pub const P9_GETATTR_MODE: u64 = 0x00000001;
pub const P9_GETATTR_NLINK: u64 = 0x00000002;
pub const P9_GETATTR_UID: u64 = 0x00000004;
pub const P9_GETATTR_GID: u64 = 0x00000008;
pub const P9_GETATTR_RDEV: u64 = 0x00000010;
pub const P9_GETATTR_ATIME: u64 = 0x00000020;
pub const P9_GETATTR_MTIME: u64 = 0x00000040;
pub const P9_GETATTR_CTIME: u64 = 0x00000080;
pub const P9_GETATTR_INO: u64 = 0x00000100;
pub const P9_GETATTR_SIZE: u64 = 0x00000200;
pub const P9_GETATTR_BLOCKS: u64 = 0x00000400;
pub const P9_GETATTR_BTIME: u64 = 0x00000800;
pub const P9_GETATTR_GEN: u64 = 0x00001000;
pub const P9_GETATTR_DATA_VERSION: u64 = 0x00002000;
pub const P9_GETATTR_BASIC: u64 = 0x000007ff; // All except btime/gen/data_version

// Open flags (Linux compatible)
pub const P9_RDONLY: u32 = 0;
pub const P9_WRONLY: u32 = 1;
pub const P9_RDWR: u32 = 2;
pub const P9_NOACCESS: u32 = 3;
pub const P9_CREATE: u32 = 0x40;
pub const P9_EXCL: u32 = 0x80;
pub const P9_NOCTTY: u32 = 0x100;
pub const P9_TRUNC: u32 = 0x200;
pub const P9_APPEND: u32 = 0x400;
pub const P9_NONBLOCK: u32 = 0x800;
pub const P9_DSYNC: u32 = 0x1000;
pub const P9_FASYNC: u32 = 0x2000;
pub const P9_DIRECT: u32 = 0x4000;
pub const P9_LARGEFILE: u32 = 0x8000;
pub const P9_DIRECTORY: u32 = 0x10000;
pub const P9_NOFOLLOW: u32 = 0x20000;
pub const P9_NOATIME: u32 = 0x40000;
pub const P9_CLOEXEC: u32 = 0x80000;
pub const P9_SYNC: u32 = 0x101000;

// Special FIDs
pub const NOFID: u32 = u32::MAX;

/// QID - unique file identifier
#[repr(C, packed)]
#[derive(Debug, Clone, Copy, Default)]
pub struct Qid {
    pub typ: u8,
    pub version: u32,
    pub path: u64,
}

impl Qid {
    pub const SIZE: usize = 13;

    pub fn decode(data: &[u8]) -> Option<Self> {
        if data.len() < Self::SIZE {
            return None;
        }
        Some(Self {
            typ: data[0],
            version: u32::from_le_bytes([data[1], data[2], data[3], data[4]]),
            path: u64::from_le_bytes([
                data[5], data[6], data[7], data[8], data[9], data[10], data[11], data[12],
            ]),
        })
    }

    pub fn encode(&self, buf: &mut [u8]) {
        buf[0] = self.typ;
        buf[1..5].copy_from_slice(&self.version.to_le_bytes());
        buf[5..13].copy_from_slice(&self.path.to_le_bytes());
    }

    pub fn is_dir(&self) -> bool {
        self.typ & QID_DIR != 0
    }
}

/// 9P message header
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct Header {
    pub size: u32,
    pub typ: u8,
    pub tag: u16,
}

impl Header {
    pub const SIZE: usize = 7;

    pub fn new(typ: MsgType, tag: u16) -> Self {
        Self {
            size: 0, // Will be filled in later
            typ: typ as u8,
            tag,
        }
    }

    pub fn decode(data: &[u8]) -> Option<Self> {
        if data.len() < Self::SIZE {
            return None;
        }
        Some(Self {
            size: u32::from_le_bytes([data[0], data[1], data[2], data[3]]),
            typ: data[4],
            tag: u16::from_le_bytes([data[5], data[6]]),
        })
    }

    pub fn encode(&self, buf: &mut [u8]) {
        buf[0..4].copy_from_slice(&self.size.to_le_bytes());
        buf[4] = self.typ;
        buf[5..7].copy_from_slice(&self.tag.to_le_bytes());
    }
}

/// Message builder for outgoing 9P messages
pub struct MessageBuilder {
    buf: Vec<u8>,
    tag: u16,
}

impl MessageBuilder {
    pub fn new(typ: MsgType, tag: u16) -> Self {
        let mut buf = vec![0u8; Header::SIZE];
        let header = Header::new(typ, tag);
        header.encode(&mut buf);
        Self { buf, tag }
    }

    pub fn put_u8(mut self, v: u8) -> Self {
        self.buf.push(v);
        self
    }

    pub fn put_u16(mut self, v: u16) -> Self {
        self.buf.extend_from_slice(&v.to_le_bytes());
        self
    }

    pub fn put_u32(mut self, v: u32) -> Self {
        self.buf.extend_from_slice(&v.to_le_bytes());
        self
    }

    pub fn put_u64(mut self, v: u64) -> Self {
        self.buf.extend_from_slice(&v.to_le_bytes());
        self
    }

    pub fn put_str(mut self, s: &str) -> Self {
        let len = s.len() as u16;
        self.buf.extend_from_slice(&len.to_le_bytes());
        self.buf.extend_from_slice(s.as_bytes());
        self
    }

    pub fn put_data(mut self, data: &[u8]) -> Self {
        let len = data.len() as u32;
        self.buf.extend_from_slice(&len.to_le_bytes());
        self.buf.extend_from_slice(data);
        self
    }

    pub fn put_qid(mut self, qid: &Qid) -> Self {
        let mut tmp = [0u8; Qid::SIZE];
        qid.encode(&mut tmp);
        self.buf.extend_from_slice(&tmp);
        self
    }

    pub fn finish(mut self) -> Vec<u8> {
        let size = self.buf.len() as u32;
        self.buf[0..4].copy_from_slice(&size.to_le_bytes());
        self.buf
    }
}

/// Message parser for incoming 9P messages
pub struct MessageParser<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> MessageParser<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    pub fn skip(&mut self, n: usize) -> Option<()> {
        if self.pos + n > self.data.len() {
            return None;
        }
        self.pos += n;
        Some(())
    }

    pub fn remaining(&self) -> &'a [u8] {
        &self.data[self.pos..]
    }

    pub fn get_u8(&mut self) -> Option<u8> {
        if self.pos >= self.data.len() {
            return None;
        }
        let v = self.data[self.pos];
        self.pos += 1;
        Some(v)
    }

    pub fn get_u16(&mut self) -> Option<u16> {
        if self.pos + 2 > self.data.len() {
            return None;
        }
        let v = u16::from_le_bytes([self.data[self.pos], self.data[self.pos + 1]]);
        self.pos += 2;
        Some(v)
    }

    pub fn get_u32(&mut self) -> Option<u32> {
        if self.pos + 4 > self.data.len() {
            return None;
        }
        let v = u32::from_le_bytes([
            self.data[self.pos],
            self.data[self.pos + 1],
            self.data[self.pos + 2],
            self.data[self.pos + 3],
        ]);
        self.pos += 4;
        Some(v)
    }

    pub fn get_u64(&mut self) -> Option<u64> {
        if self.pos + 8 > self.data.len() {
            return None;
        }
        let v = u64::from_le_bytes([
            self.data[self.pos],
            self.data[self.pos + 1],
            self.data[self.pos + 2],
            self.data[self.pos + 3],
            self.data[self.pos + 4],
            self.data[self.pos + 5],
            self.data[self.pos + 6],
            self.data[self.pos + 7],
        ]);
        self.pos += 8;
        Some(v)
    }

    pub fn get_str(&mut self) -> Option<&'a str> {
        let len = self.get_u16()? as usize;
        if self.pos + len > self.data.len() {
            return None;
        }
        let s = core::str::from_utf8(&self.data[self.pos..self.pos + len]).ok()?;
        self.pos += len;
        Some(s)
    }

    pub fn get_data(&mut self) -> Option<&'a [u8]> {
        let len = self.get_u32()? as usize;
        if self.pos + len > self.data.len() {
            return None;
        }
        let d = &self.data[self.pos..self.pos + len];
        self.pos += len;
        Some(d)
    }

    pub fn get_qid(&mut self) -> Option<Qid> {
        if self.pos + Qid::SIZE > self.data.len() {
            return None;
        }
        let qid = Qid::decode(&self.data[self.pos..])?;
        self.pos += Qid::SIZE;
        Some(qid)
    }

    pub fn get_header(&mut self) -> Option<Header> {
        if self.pos + Header::SIZE > self.data.len() {
            return None;
        }
        let header = Header::decode(&self.data[self.pos..])?;
        self.pos += Header::SIZE;
        Some(header)
    }
}

/// File attributes from Rgetattr
#[derive(Debug, Clone, Default)]
pub struct FileAttr {
    pub valid: u64,
    pub qid: Qid,
    pub mode: u32,
    pub uid: u32,
    pub gid: u32,
    pub nlink: u64,
    pub rdev: u64,
    pub size: u64,
    pub blksize: u64,
    pub blocks: u64,
    pub atime_sec: u64,
    pub atime_nsec: u64,
    pub mtime_sec: u64,
    pub mtime_nsec: u64,
    pub ctime_sec: u64,
    pub ctime_nsec: u64,
    pub btime_sec: u64,
    pub btime_nsec: u64,
    pub gen: u64,
    pub data_version: u64,
}

impl FileAttr {
    pub fn decode(parser: &mut MessageParser) -> Option<Self> {
        Some(Self {
            valid: parser.get_u64()?,
            qid: parser.get_qid()?,
            mode: parser.get_u32()?,
            uid: parser.get_u32()?,
            gid: parser.get_u32()?,
            nlink: parser.get_u64()?,
            rdev: parser.get_u64()?,
            size: parser.get_u64()?,
            blksize: parser.get_u64()?,
            blocks: parser.get_u64()?,
            atime_sec: parser.get_u64()?,
            atime_nsec: parser.get_u64()?,
            mtime_sec: parser.get_u64()?,
            mtime_nsec: parser.get_u64()?,
            ctime_sec: parser.get_u64()?,
            ctime_nsec: parser.get_u64()?,
            btime_sec: parser.get_u64()?,
            btime_nsec: parser.get_u64()?,
            gen: parser.get_u64()?,
            data_version: parser.get_u64()?,
        })
    }
}

/// Statfs result
#[derive(Debug, Clone, Default)]
pub struct StatFs {
    pub typ: u32,
    pub bsize: u32,
    pub blocks: u64,
    pub bfree: u64,
    pub bavail: u64,
    pub files: u64,
    pub ffree: u64,
    pub fsid: u64,
    pub namelen: u32,
}

impl StatFs {
    pub fn decode(parser: &mut MessageParser) -> Option<Self> {
        Some(Self {
            typ: parser.get_u32()?,
            bsize: parser.get_u32()?,
            blocks: parser.get_u64()?,
            bfree: parser.get_u64()?,
            bavail: parser.get_u64()?,
            files: parser.get_u64()?,
            ffree: parser.get_u64()?,
            fsid: parser.get_u64()?,
            namelen: parser.get_u32()?,
        })
    }
}

/// Directory entry from Rreaddir
#[derive(Debug, Clone)]
pub struct DirEntry {
    pub qid: Qid,
    pub offset: u64,
    pub typ: u8,
    pub name: String,
}

impl DirEntry {
    pub fn decode(parser: &mut MessageParser) -> Option<Self> {
        Some(Self {
            qid: parser.get_qid()?,
            offset: parser.get_u64()?,
            typ: parser.get_u8()?,
            name: parser.get_str()?.to_string(),
        })
    }
}
