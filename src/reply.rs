use crate::{
    abi::{
        fuse_attr, //
        fuse_attr_out,
        fuse_entry_out,
        fuse_init_out,
        fuse_open_out,
        fuse_out_header,
        FUSE_KERNEL_MINOR_VERSION,
        FUSE_KERNEL_VERSION,
    },
    request::{CapFlags, InHeader},
    util::{AsyncWriteVectored, AsyncWriteVectoredExt},
};
use std::{
    io::{self, IoSlice},
    mem,
};

const OUT_HEADER_SIZE: usize = mem::size_of::<fuse_out_header>();

#[repr(transparent)]
pub struct Attr(pub(crate) fuse_attr);

impl Default for Attr {
    fn default() -> Self {
        unsafe { mem::zeroed() }
    }
}

impl From<libc::stat> for Attr {
    fn from(attr: libc::stat) -> Self {
        Self(fuse_attr {
            ino: attr.st_ino,
            mode: attr.st_mode,
            nlink: attr.st_nlink as u32,
            uid: attr.st_uid,
            gid: attr.st_gid,
            rdev: attr.st_gid,
            size: attr.st_size as u64,
            blksize: attr.st_blksize as u32,
            blocks: attr.st_blocks as u64,
            atime: attr.st_atime as u64,
            mtime: attr.st_mtime as u64,
            ctime: attr.st_ctime as u64,
            atimensec: attr.st_atime_nsec as u32,
            mtimensec: attr.st_mtime_nsec as u32,
            ctimensec: attr.st_ctime_nsec as u32,
            padding: 0,
        })
    }
}

#[repr(transparent)]
pub struct AttrOut(fuse_attr_out);

impl Default for AttrOut {
    fn default() -> Self {
        unsafe { mem::zeroed() }
    }
}

impl From<Attr> for AttrOut {
    fn from(attr: Attr) -> Self {
        let mut attr_out = Self::default();
        attr_out.set_attr(attr);
        attr_out
    }
}

impl From<libc::stat> for AttrOut {
    fn from(attr: libc::stat) -> Self {
        Self::from(Attr::from(attr))
    }
}

impl AttrOut {
    pub fn set_attr(&mut self, attr: impl Into<Attr>) {
        self.0.attr = attr.into().0;
    }

    pub fn set_attr_valid(&mut self, sec: u64, nsec: u32) {
        self.0.attr_valid = sec;
        self.0.attr_valid_nsec = nsec;
    }
}

#[repr(transparent)]
pub struct EntryOut(pub(crate) fuse_entry_out);

impl Default for EntryOut {
    fn default() -> Self {
        Self(fuse_entry_out {
            nodeid: 0,
            generation: 0,
            entry_valid: 0,
            attr_valid: 0,
            entry_valid_nsec: 0,
            attr_valid_nsec: 0,
            attr: Attr::default().0,
        })
    }
}

impl EntryOut {
    pub fn set_nodeid(&mut self, nodeid: u64) {
        self.0.nodeid = nodeid;
    }

    pub fn set_generation(&mut self, generation: u64) {
        self.0.generation = generation;
    }

    pub fn set_entry_valid(&mut self, sec: u64, nsec: u32) {
        self.0.entry_valid = sec;
        self.0.entry_valid_nsec = nsec;
    }

    pub fn set_attr_valid(&mut self, sec: u64, nsec: u32) {
        self.0.attr_valid = sec;
        self.0.attr_valid_nsec = nsec;
    }

    pub fn set_attr(&mut self, attr: impl Into<Attr>) {
        self.0.attr = attr.into().0;
    }
}

#[repr(transparent)]
pub struct InitOut(fuse_init_out);

impl Default for InitOut {
    fn default() -> Self {
        let mut init_out: fuse_init_out = unsafe { mem::zeroed() };
        init_out.major = FUSE_KERNEL_VERSION;
        init_out.minor = FUSE_KERNEL_MINOR_VERSION;
        Self(init_out)
    }
}

impl InitOut {
    pub fn set_flags(&mut self, flags: CapFlags) {
        self.0.flags = flags.bits();
    }

    pub fn max_readahead(&self) -> u32 {
        self.0.max_readahead
    }

    pub fn set_max_readahead(&mut self, max_readahead: u32) {
        self.0.max_readahead = max_readahead;
    }

    pub fn max_write(&self) -> u32 {
        self.0.max_write
    }

    pub fn set_max_write(&mut self, max_write: u32) {
        self.0.max_write = max_write;
    }
}

#[repr(transparent)]
pub struct OpenOut(fuse_open_out);

impl Default for OpenOut {
    fn default() -> Self {
        unsafe { mem::zeroed() }
    }
}

pub trait Payload {
    unsafe fn to_io_slice(&self) -> IoSlice<'_>;
}

impl Payload for [u8] {
    unsafe fn to_io_slice(&self) -> IoSlice<'_> {
        IoSlice::new(&*self)
    }
}

macro_rules! impl_payload_for_abi {
    ($($t:ty,)*) => {$(
        impl Payload for $t {
            unsafe fn to_io_slice(&self) -> IoSlice<'_> {
                IoSlice::new(std::slice::from_raw_parts(
                    self as *const Self as *const u8,
                    mem::size_of::<Self>(),
                ))
            }
        }
    )*}
}

impl_payload_for_abi! {
    fuse_out_header,
    InitOut,
    OpenOut,
    AttrOut,
    EntryOut,
}

pub async fn reply_payload<'a, W: ?Sized, T: ?Sized>(
    writer: &'a mut W,
    in_header: &'a InHeader,
    error: i32,
    data: &'a T,
) -> io::Result<()>
where
    W: AsyncWriteVectored + Unpin,
    T: Payload,
{
    let data = unsafe { data.to_io_slice() };

    let mut out_header: fuse_out_header = unsafe { mem::zeroed() };
    out_header.unique = in_header.unique();
    out_header.error = -error;
    out_header.len = (OUT_HEADER_SIZE + data.len()) as u32;

    let out_header = unsafe { out_header.to_io_slice() };
    (*writer).write_vectored(&[out_header, data]).await?;

    Ok(())
}

pub async fn reply_none<'a, W: ?Sized>(writer: &'a mut W, in_header: &'a InHeader) -> io::Result<()>
where
    W: AsyncWriteVectored + Unpin,
{
    reply_payload(writer, in_header, 0, &[] as &[u8]).await
}

pub async fn reply_err<'a, W: ?Sized>(
    writer: &'a mut W,
    in_header: &'a InHeader,
    error: i32,
) -> io::Result<()>
where
    W: AsyncWriteVectored + Unpin,
{
    reply_payload(writer, in_header, error, &[] as &[u8]).await
}