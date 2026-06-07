//! `std::fs` subset: `File`, `OpenOptions`, and the free helpers `read`,
//! `read_to_string`, `write`, `remove_file`, `create_dir`. Backed by
//! `openat`/`read`/`write`/`close`.

use crate::alloc::string::String;
use crate::alloc::vec::Vec;
use crate::ffi::CString;
use crate::io::{self, Error, ErrorKind, Read, Write};
use crate::path::Path;
use crate::syscall;

fn cpath(path: &Path) -> io::Result<CString> {
    CString::new(path.as_str().as_bytes()).map_err(|_| Error::from(ErrorKind::InvalidInput))
}

/// An open file. Closes its descriptor on drop. Drop-in for `std::fs::File`.
pub struct File {
    fd: i32,
}

impl File {
    /// Open a file in read-only mode.
    pub fn open<P: AsRef<Path>>(path: P) -> io::Result<File> {
        OpenOptions::new().read(true).open(path)
    }

    /// Open a file for writing, creating it (truncating if it exists).
    pub fn create<P: AsRef<Path>>(path: P) -> io::Result<File> {
        OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(path)
    }

    /// The raw file descriptor.
    pub fn as_raw_fd(&self) -> i32 {
        self.fd
    }

    /// Truncate or extend the file to `size` bytes.
    pub fn set_len(&self, size: u64) -> io::Result<()> {
        syscall::ftruncate(self.fd, size).map_err(Error::from)
    }

    /// Flush all in-memory data and metadata to disk.
    pub fn sync_all(&self) -> io::Result<()> {
        syscall::fsync(self.fd).map_err(Error::from)
    }

    /// Flush in-memory data to disk. (We don't split data/metadata; same as
    /// `sync_all`.)
    pub fn sync_data(&self) -> io::Result<()> {
        syscall::fsync(self.fd).map_err(Error::from)
    }
}

impl Drop for File {
    fn drop(&mut self) {
        let _ = syscall::close(self.fd);
    }
}

impl Read for File {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        io::Fd(self.fd).read(buf)
    }
    fn read_vectored(&mut self, bufs: &mut [io::IoSliceMut<'_>]) -> io::Result<usize> {
        io::Fd(self.fd).read_vectored(bufs)
    }
}

impl Write for File {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        io::Fd(self.fd).write(buf)
    }
    fn write_vectored(&mut self, bufs: &[io::IoSlice<'_>]) -> io::Result<usize> {
        io::Fd(self.fd).write_vectored(bufs)
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl io::Seek for File {
    fn seek(&mut self, pos: io::SeekFrom) -> io::Result<u64> {
        io::Seek::seek(&mut io::Fd(self.fd), pos)
    }
}

/// Options and flags configuring how a file is opened. Drop-in for
/// `std::fs::OpenOptions`.
#[derive(Clone, Debug, Default)]
pub struct OpenOptions {
    read: bool,
    write: bool,
    append: bool,
    truncate: bool,
    create: bool,
}

impl OpenOptions {
    pub fn new() -> OpenOptions {
        OpenOptions::default()
    }
    pub fn read(&mut self, v: bool) -> &mut Self {
        self.read = v;
        self
    }
    pub fn write(&mut self, v: bool) -> &mut Self {
        self.write = v;
        self
    }
    pub fn append(&mut self, v: bool) -> &mut Self {
        self.append = v;
        self
    }
    pub fn truncate(&mut self, v: bool) -> &mut Self {
        self.truncate = v;
        self
    }
    pub fn create(&mut self, v: bool) -> &mut Self {
        self.create = v;
        self
    }

    pub fn open<P: AsRef<Path>>(&self, path: P) -> io::Result<File> {
        let c = cpath(path.as_ref())?;
        let mut flags = match (self.read, self.write || self.append) {
            (true, true) => syscall::O_RDWR,
            (false, true) => syscall::O_WRONLY,
            _ => syscall::O_RDONLY,
        };
        if self.create {
            flags |= syscall::O_CREAT;
        }
        if self.truncate {
            flags |= syscall::O_TRUNC;
        }
        if self.append {
            flags |= syscall::O_APPEND;
        }
        let fd = syscall::open(&c, flags, 0o644).map_err(Error::from)?;
        Ok(File { fd })
    }
}

/// Read the entire contents of a file into a bytes vector.
pub fn read<P: AsRef<Path>>(path: P) -> io::Result<Vec<u8>> {
    let mut f = File::open(path)?;
    let mut buf = Vec::new();
    f.read_to_end(&mut buf)?;
    Ok(buf)
}

/// Read the entire contents of a file into a string.
pub fn read_to_string<P: AsRef<Path>>(path: P) -> io::Result<String> {
    let mut f = File::open(path)?;
    let mut s = String::new();
    f.read_to_string(&mut s)?;
    Ok(s)
}

/// Write a slice as the entire contents of a file, creating/truncating it.
pub fn write<P: AsRef<Path>, C: AsRef<[u8]>>(path: P, contents: C) -> io::Result<()> {
    let mut f = File::create(path)?;
    f.write_all(contents.as_ref())
}

/// Remove a file.
pub fn remove_file<P: AsRef<Path>>(path: P) -> io::Result<()> {
    let c = cpath(path.as_ref())?;
    syscall::unlink(&c).map_err(Error::from)
}

/// Create a directory.
pub fn create_dir<P: AsRef<Path>>(path: P) -> io::Result<()> {
    let c = cpath(path.as_ref())?;
    syscall::mkdir(&c, 0o755).map_err(Error::from)
}

impl crate::os::fd::AsRawFd for File {
    fn as_raw_fd(&self) -> i32 {
        self.fd
    }
}

// ---------------------------------------------------------------------------
// Metadata / FileType / Permissions
// ---------------------------------------------------------------------------

const S_IFMT: u32 = 0o170000;
const S_IFREG: u32 = 0o100000;
const S_IFDIR: u32 = 0o040000;
const S_IFLNK: u32 = 0o120000;

// Field offsets into the kernel `struct stat`, which differs per target.
#[cfg(target_os = "macos")]
fn st_mode(b: &syscall::StatBuf) -> u32 {
    u16::from_ne_bytes([b[4], b[5]]) as u32
}
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn st_mode(b: &syscall::StatBuf) -> u32 {
    u32::from_ne_bytes([b[24], b[25], b[26], b[27]])
}
// st_mode sits at offset 16 on every Linux layout except x86-64 (asm-generic
// `struct stat` and the 32-bit `struct stat64` agree here).
#[cfg(all(
    target_os = "linux",
    any(
        target_arch = "aarch64",
        target_arch = "riscv64",
        target_arch = "x86",
        target_arch = "arm"
    )
))]
fn st_mode(b: &syscall::StatBuf) -> u32 {
    u32::from_ne_bytes([b[16], b[17], b[18], b[19]])
}

// st_size: 64-bit on every target. On 32-bit Linux this is `struct stat64`,
// whose `long long` alignment differs — i386 packs it at 44 (4-byte aligned),
// ARM EABI at 48 (8-byte aligned).
#[cfg(target_os = "macos")]
const ST_SIZE_OFF: usize = 96;
#[cfg(all(target_os = "linux", target_arch = "x86"))]
const ST_SIZE_OFF: usize = 44;
#[cfg(all(target_os = "linux", not(target_arch = "x86")))]
const ST_SIZE_OFF: usize = 48;

// st_mtime tv_sec. 64-bit (`read_i64`) on 64-bit targets; a 32-bit field in the
// 32-bit `struct stat64`, with i386 and ARM at different offsets.
#[cfg(target_os = "macos")]
const ST_MTIME_OFF: usize = 48;
#[cfg(all(target_os = "linux", target_arch = "x86"))]
const ST_MTIME_OFF: usize = 72;
#[cfg(all(target_os = "linux", target_arch = "arm"))]
const ST_MTIME_OFF: usize = 80;
#[cfg(all(target_os = "linux", not(any(target_arch = "x86", target_arch = "arm"))))]
const ST_MTIME_OFF: usize = 88;

fn read_i64(b: &[u8], off: usize) -> i64 {
    let mut a = [0u8; 8];
    a.copy_from_slice(&b[off..off + 8]);
    i64::from_ne_bytes(a)
}

/// Read an `st_*time` seconds field: 32-bit in the 32-bit `struct stat64`,
/// 64-bit elsewhere.
fn read_time_secs(b: &[u8], off: usize) -> i64 {
    #[cfg(all(target_os = "linux", target_pointer_width = "32"))]
    {
        u32::from_ne_bytes([b[off], b[off + 1], b[off + 2], b[off + 3]]) as i64
    }
    #[cfg(not(all(target_os = "linux", target_pointer_width = "32")))]
    {
        read_i64(b, off)
    }
}

/// The type of a file (regular/dir/symlink). Drop-in for `std::fs::FileType`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct FileType(u32);
impl FileType {
    pub fn is_file(&self) -> bool {
        self.0 & S_IFMT == S_IFREG
    }
    pub fn is_dir(&self) -> bool {
        self.0 & S_IFMT == S_IFDIR
    }
    pub fn is_symlink(&self) -> bool {
        self.0 & S_IFMT == S_IFLNK
    }
}

/// File permissions. Drop-in for `std::fs::Permissions`.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Permissions(u32);
impl Permissions {
    pub fn readonly(&self) -> bool {
        self.0 & 0o222 == 0
    }
    pub fn set_readonly(&mut self, readonly: bool) {
        if readonly {
            self.0 &= !0o222;
        } else {
            self.0 |= 0o200;
        }
    }
    pub fn mode(&self) -> u32 {
        self.0
    }
}

/// Metadata for a file. Drop-in for `std::fs::Metadata`.
#[derive(Clone)]
pub struct Metadata {
    raw: syscall::StatBuf,
}
impl Metadata {
    pub fn file_type(&self) -> FileType {
        FileType(st_mode(&self.raw) & S_IFMT)
    }
    pub fn is_file(&self) -> bool {
        self.file_type().is_file()
    }
    pub fn is_dir(&self) -> bool {
        self.file_type().is_dir()
    }
    pub fn is_symlink(&self) -> bool {
        self.file_type().is_symlink()
    }
    pub fn len(&self) -> u64 {
        read_i64(&self.raw, ST_SIZE_OFF) as u64
    }
    pub fn permissions(&self) -> Permissions {
        Permissions(st_mode(&self.raw) & 0o7777)
    }
    pub fn modified(&self) -> io::Result<crate::time::SystemTime> {
        let secs = read_time_secs(&self.raw, ST_MTIME_OFF);
        Ok(crate::time::UNIX_EPOCH + crate::time::Duration::from_secs(secs as u64))
    }
}

impl File {
    /// Query metadata for this open file.
    pub fn metadata(&self) -> io::Result<Metadata> {
        Ok(Metadata {
            raw: syscall::fstat(self.fd).map_err(Error::from)?,
        })
    }
}

/// Metadata for the file at `path` (follows symlinks).
pub fn metadata<P: AsRef<Path>>(path: P) -> io::Result<Metadata> {
    let c = cpath(path.as_ref())?;
    Ok(Metadata {
        raw: syscall::statat(&c, true).map_err(Error::from)?,
    })
}

/// Metadata for the file at `path` without following a final symlink.
pub fn symlink_metadata<P: AsRef<Path>>(path: P) -> io::Result<Metadata> {
    let c = cpath(path.as_ref())?;
    Ok(Metadata {
        raw: syscall::statat(&c, false).map_err(Error::from)?,
    })
}

// ---------------------------------------------------------------------------
// rename / copy / dirs
// ---------------------------------------------------------------------------

/// Rename a file or directory.
pub fn rename<P: AsRef<Path>, Q: AsRef<Path>>(from: P, to: Q) -> io::Result<()> {
    let a = cpath(from.as_ref())?;
    let b = cpath(to.as_ref())?;
    syscall::rename(&a, &b).map_err(Error::from)
}

/// Copy the contents of `from` to `to`, returning the number of bytes copied.
pub fn copy<P: AsRef<Path>, Q: AsRef<Path>>(from: P, to: Q) -> io::Result<u64> {
    let mut src = File::open(from)?;
    let mut dst = File::create(to)?;
    io::copy(&mut src, &mut dst)
}

/// Remove an empty directory.
pub fn remove_dir<P: AsRef<Path>>(path: P) -> io::Result<()> {
    let c = cpath(path.as_ref())?;
    syscall::rmdir(&c).map_err(Error::from)
}

/// Recursively create a directory and all of its parents.
pub fn create_dir_all<P: AsRef<Path>>(path: P) -> io::Result<()> {
    let p = path.as_ref();
    if metadata(p).map(|m| m.is_dir()).unwrap_or(false) {
        return Ok(());
    }
    if let Some(parent) = p.parent() {
        if !parent.as_str().is_empty() {
            create_dir_all(parent)?;
        }
    }
    match create_dir(p) {
        Ok(()) => Ok(()),
        Err(ref e) if e.kind() == ErrorKind::AlreadyExists => Ok(()),
        Err(e) => Err(e),
    }
}

/// Recursively remove a directory and all its contents.
pub fn remove_dir_all<P: AsRef<Path>>(path: P) -> io::Result<()> {
    let p = path.as_ref();
    for entry in read_dir(p)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            remove_dir_all(entry.path())?;
        } else {
            remove_file(entry.path())?;
        }
    }
    remove_dir(p)
}

// ---------------------------------------------------------------------------
// read_dir
// ---------------------------------------------------------------------------

const DT_DIR: u8 = 4;
const DT_REG: u8 = 8;
const DT_LNK: u8 = 10;

/// An entry returned by [`read_dir`]. Drop-in for `std::fs::DirEntry`.
pub struct DirEntry {
    name: String,
    d_type: u8,
    parent: crate::path::PathBuf,
}
impl DirEntry {
    pub fn file_name(&self) -> crate::ffi::OsString {
        crate::ffi::OsString::from(self.name.clone())
    }
    pub fn path(&self) -> crate::path::PathBuf {
        self.parent.join(self.name.as_str())
    }
    pub fn file_type(&self) -> io::Result<FileType> {
        let t = match self.d_type {
            DT_DIR => S_IFDIR,
            DT_REG => S_IFREG,
            DT_LNK => S_IFLNK,
            _ => return Ok(symlink_metadata(self.path())?.file_type()),
        };
        Ok(FileType(t))
    }
    pub fn metadata(&self) -> io::Result<Metadata> {
        symlink_metadata(self.path())
    }
}

/// Iterator over the entries in a directory. Drop-in for `std::fs::ReadDir`.
pub struct ReadDir {
    fd: i32,
    parent: crate::path::PathBuf,
    buf: Vec<u8>,
    pos: usize,
    cap: usize,
}
impl Drop for ReadDir {
    fn drop(&mut self) {
        let _ = syscall::close(self.fd);
    }
}

/// Iterate over the entries within a directory.
pub fn read_dir<P: AsRef<Path>>(path: P) -> io::Result<ReadDir> {
    let c = cpath(path.as_ref())?;
    let fd = syscall::open(&c, syscall::O_RDONLY, 0).map_err(Error::from)?;
    let mut buf = Vec::with_capacity(4096);
    buf.resize(4096, 0);
    Ok(ReadDir {
        fd,
        parent: crate::path::PathBuf::from(path.as_ref().as_str()),
        buf,
        pos: 0,
        cap: 0,
    })
}

impl Iterator for ReadDir {
    type Item = io::Result<DirEntry>;
    fn next(&mut self) -> Option<io::Result<DirEntry>> {
        loop {
            if self.pos >= self.cap {
                match syscall::getdirentries(self.fd, &mut self.buf) {
                    Ok(0) => return None,
                    Ok(n) => {
                        self.cap = n;
                        self.pos = 0;
                    }
                    Err(e) => return Some(Err(Error::from(e))),
                }
            }
            let rec = &self.buf[self.pos..self.cap];
            // d_reclen is at offset 16 on both Linux (dirent64) and macOS (dirent).
            let reclen = u16::from_ne_bytes([rec[16], rec[17]]) as usize;
            if reclen == 0 || self.pos + reclen > self.cap {
                // Defensive: force a refill.
                self.pos = self.cap;
                continue;
            }
            #[cfg(target_os = "macos")]
            let (name_off, d_type, namlen) = {
                let namlen = u16::from_ne_bytes([rec[18], rec[19]]) as usize;
                (21usize, rec[20], Some(namlen))
            };
            #[cfg(not(target_os = "macos"))]
            let (name_off, d_type, namlen): (usize, u8, Option<usize>) = (19, rec[18], None);

            let name_bytes = match namlen {
                Some(l) => &rec[name_off..name_off + l],
                None => {
                    let end = rec[name_off..reclen]
                        .iter()
                        .position(|&b| b == 0)
                        .map(|i| name_off + i)
                        .unwrap_or(reclen);
                    &rec[name_off..end]
                }
            };
            let name = String::from_utf8_lossy(name_bytes).into_owned();
            self.pos += reclen;
            if name == "." || name == ".." {
                continue;
            }
            return Some(Ok(DirEntry {
                name,
                d_type,
                parent: self.parent.clone(),
            }));
        }
    }
}

// Raw stat accessors for os::unix::fs::MetadataExt (uid/gid offsets per target).
#[cfg(target_os = "macos")]
const ST_UID_OFF: usize = 16;
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
const ST_UID_OFF: usize = 28;
#[cfg(all(
    target_os = "linux",
    any(
        target_arch = "aarch64",
        target_arch = "riscv64",
        target_arch = "x86",
        target_arch = "arm"
    )
))]
const ST_UID_OFF: usize = 24;

impl Metadata {
    #[doc(hidden)]
    pub fn raw_mode(&self) -> u32 {
        st_mode(&self.raw)
    }
    #[doc(hidden)]
    pub fn raw_uid(&self) -> u32 {
        u32::from_ne_bytes([
            self.raw[ST_UID_OFF],
            self.raw[ST_UID_OFF + 1],
            self.raw[ST_UID_OFF + 2],
            self.raw[ST_UID_OFF + 3],
        ])
    }
    #[doc(hidden)]
    pub fn raw_gid(&self) -> u32 {
        let o = ST_UID_OFF + 4;
        u32::from_ne_bytes([self.raw[o], self.raw[o + 1], self.raw[o + 2], self.raw[o + 3]])
    }
    #[doc(hidden)]
    pub fn raw_ino(&self) -> u64 {
        // st_ino is a u64 at offset 8 on macOS and both Linux arches.
        let mut a = [0u8; 8];
        a.copy_from_slice(&self.raw[8..16]);
        u64::from_ne_bytes(a)
    }
}

impl Permissions {
    #[doc(hidden)]
    pub fn from_mode_raw(mode: u32) -> Permissions {
        Permissions(mode & 0o7777)
    }
}
