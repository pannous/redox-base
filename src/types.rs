pub const MAGIC_LEN: usize = 8;
pub const MAGIC: [u8; 8] = *b"RedoxFtw";

#[repr(transparent)]
#[derive(Clone, Copy, Debug)]
pub struct Magic(pub [u8; MAGIC_LEN]);

#[repr(transparent)]
#[derive(Clone, Copy, Debug)]
pub struct Offset(pub u32);

#[repr(transparent)]
#[derive(Clone, Copy, Debug)]
pub struct Length(pub u32);

#[repr(transparent)]
#[derive(Clone, Copy, Debug)]
pub struct Inode(pub u16);

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct Timespec {
    pub sec: u64,
    pub nsec: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct Header {
    pub magic: Magic,
    pub inode_table_offset: Offset,
    pub creation_time: Timespec,
    pub inode_count: u16,
}

#[repr(C)]
pub struct InodeHeader {
    pub type_and_mode: u32,
    pub length: u32,
    pub offset: Offset,
    pub uid: u32,
    pub gid: u32,
}

#[repr(C)]
pub struct DirEntry {
    pub inode: Inode,
    pub name_len: u16,
    pub name_offset: Offset,
}
