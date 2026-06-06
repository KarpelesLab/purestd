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
}

impl Write for File {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        io::Fd(self.fd).write(buf)
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
