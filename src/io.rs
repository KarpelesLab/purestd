//! A drop-in subset of `std::io`: `Error`/`ErrorKind`/`Result`, the `Read` and
//! `Write` traits with their common provided methods, and the standard streams
//! `Stdin`/`Stdout`/`Stderr`.
//!
//! The low-level [`Fd`] handle wraps a raw file descriptor and is what `fs` and
//! the streams are built on. Everything is backed directly by [`crate::syscall`].

use crate::alloc::string::String;
use crate::alloc::vec::Vec;
use crate::syscall::{self, Errno};
use core::fmt;

pub const STDIN: i32 = 0;
pub const STDOUT: i32 = 1;
pub const STDERR: i32 = 2;

/// The `std::io::prelude` — the I/O traits, for `use io::prelude::*`.
pub mod prelude {
    pub use super::{BufRead, Read, Seek, Write};
}

/// Specialized `Result` for I/O operations. Drop-in for `std::io::Result`.
pub type Result<T> = core::result::Result<T, Error>;

/// A list of the categories of I/O error, mirroring `std::io::ErrorKind`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ErrorKind {
    NotFound,
    PermissionDenied,
    ConnectionRefused,
    ConnectionReset,
    ConnectionAborted,
    NotConnected,
    AddrInUse,
    AddrNotAvailable,
    BrokenPipe,
    AlreadyExists,
    WouldBlock,
    InvalidInput,
    InvalidData,
    TimedOut,
    WriteZero,
    Interrupted,
    UnexpectedEof,
    Unsupported,
    OutOfMemory,
    Other,
}

impl ErrorKind {
    fn as_str(&self) -> &'static str {
        use ErrorKind::*;
        match self {
            NotFound => "entity not found",
            PermissionDenied => "permission denied",
            ConnectionRefused => "connection refused",
            ConnectionReset => "connection reset",
            ConnectionAborted => "connection aborted",
            NotConnected => "not connected",
            AddrInUse => "address in use",
            AddrNotAvailable => "address not available",
            BrokenPipe => "broken pipe",
            AlreadyExists => "entity already exists",
            WouldBlock => "operation would block",
            InvalidInput => "invalid input parameter",
            InvalidData => "invalid data",
            TimedOut => "timed out",
            WriteZero => "write zero",
            Interrupted => "operation interrupted",
            UnexpectedEof => "unexpected end of file",
            Unsupported => "unsupported",
            OutOfMemory => "out of memory",
            Other => "other error",
        }
    }
}

enum Repr {
    Os(i32),
    Simple(ErrorKind),
    Custom(ErrorKind, crate::alloc::boxed::Box<dyn core::error::Error + Send + Sync>),
}

/// The error type for I/O operations. Drop-in for `std::io::Error`.
pub struct Error(Repr);

impl Error {
    /// Create an error from an [`ErrorKind`] and an arbitrary payload, exactly
    /// like `std::io::Error::new`.
    pub fn new<E>(kind: ErrorKind, error: E) -> Error
    where
        E: Into<crate::alloc::boxed::Box<dyn core::error::Error + Send + Sync>>,
    {
        Error(Repr::Custom(kind, error.into()))
    }

    /// Construct from a raw OS error number.
    pub fn from_raw_os_error(code: i32) -> Error {
        Error(Repr::Os(code))
    }

    /// The raw OS error, if this error came from a syscall.
    pub fn raw_os_error(&self) -> Option<i32> {
        match self.0 {
            Repr::Os(c) => Some(c),
            _ => None,
        }
    }

    /// The [`ErrorKind`] of this error.
    pub fn kind(&self) -> ErrorKind {
        match &self.0 {
            Repr::Os(c) => errno_kind(*c),
            Repr::Simple(k) => *k,
            Repr::Custom(k, _) => *k,
        }
    }
}

impl From<ErrorKind> for Error {
    fn from(k: ErrorKind) -> Error {
        Error(Repr::Simple(k))
    }
}

impl From<Errno> for Error {
    fn from(e: Errno) -> Error {
        Error(Repr::Os(e.0))
    }
}

impl fmt::Debug for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.0 {
            Repr::Os(c) => f
                .debug_struct("Os")
                .field("code", c)
                .field("kind", &errno_kind(*c))
                .finish(),
            Repr::Simple(k) => f.debug_tuple("Kind").field(k).finish(),
            Repr::Custom(k, e) => f.debug_struct("Custom").field("kind", k).field("error", e).finish(),
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.0 {
            Repr::Os(c) => write!(f, "{} (os error {})", errno_kind(*c).as_str(), c),
            Repr::Simple(k) => f.write_str(k.as_str()),
            Repr::Custom(_, e) => fmt::Display::fmt(e, f),
        }
    }
}

impl core::error::Error for Error {}

/// Map a raw errno to an [`ErrorKind`]. Many error numbers differ between Darwin
/// and Linux, so the divergent ones are split by target.
fn errno_kind(c: i32) -> ErrorKind {
    use ErrorKind::*;
    // Shared values.
    match c {
        1 => return PermissionDenied,  // EPERM
        2 => return NotFound,          // ENOENT
        4 => return Interrupted,       // EINTR
        13 => return PermissionDenied, // EACCES
        17 => return AlreadyExists,    // EEXIST
        22 => return InvalidInput,     // EINVAL
        32 => return BrokenPipe,       // EPIPE
        _ => {}
    }
    #[cfg(target_os = "macos")]
    match c {
        35 => WouldBlock,        // EAGAIN
        61 => ConnectionRefused, // ECONNREFUSED
        54 => ConnectionReset,   // ECONNRESET
        53 => ConnectionAborted, // ECONNABORTED
        57 => NotConnected,      // ENOTCONN
        48 => AddrInUse,         // EADDRINUSE
        49 => AddrNotAvailable,  // EADDRNOTAVAIL
        60 => TimedOut,          // ETIMEDOUT
        78 => Unsupported,       // ENOSYS
        _ => Other,
    }
    #[cfg(not(target_os = "macos"))]
    match c {
        11 => WouldBlock,         // EAGAIN
        111 => ConnectionRefused, // ECONNREFUSED
        104 => ConnectionReset,   // ECONNRESET
        103 => ConnectionAborted, // ECONNABORTED
        107 => NotConnected,      // ENOTCONN
        98 => AddrInUse,          // EADDRINUSE
        99 => AddrNotAvailable,   // EADDRNOTAVAIL
        110 => TimedOut,          // ETIMEDOUT
        38 => Unsupported,        // ENOSYS
        _ => Other,
    }
}

// ---------------------------------------------------------------------------
// Read / Write traits
// ---------------------------------------------------------------------------

/// The `std::io::Read` trait (core methods + the common provided ones).
pub trait Read {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize>;

    /// Read the entire input into `buf`, returning the number of bytes read.
    fn read_to_end(&mut self, buf: &mut Vec<u8>) -> Result<usize> {
        let start = buf.len();
        let mut tmp = [0u8; 4096];
        loop {
            match self.read(&mut tmp) {
                Ok(0) => break,
                Ok(n) => buf.extend_from_slice(&tmp[..n]),
                Err(ref e) if e.kind() == ErrorKind::Interrupted => continue,
                Err(e) => return Err(e),
            }
        }
        Ok(buf.len() - start)
    }

    /// Read the entire input into a UTF-8 `String`.
    fn read_to_string(&mut self, buf: &mut String) -> Result<usize> {
        let mut bytes = Vec::new();
        let n = self.read_to_end(&mut bytes)?;
        let s = core::str::from_utf8(&bytes)
            .map_err(|_| Error::from(ErrorKind::InvalidData))?;
        buf.push_str(s);
        Ok(n)
    }

    /// Read exactly enough bytes to fill `buf`.
    fn read_exact(&mut self, mut buf: &mut [u8]) -> Result<()> {
        while !buf.is_empty() {
            match self.read(buf) {
                Ok(0) => return Err(Error::from(ErrorKind::UnexpectedEof)),
                Ok(n) => buf = &mut buf[n..],
                Err(ref e) if e.kind() == ErrorKind::Interrupted => continue,
                Err(e) => return Err(e),
            }
        }
        Ok(())
    }
}

/// The `std::io::Write` trait (core methods + the common provided ones).
pub trait Write {
    fn write(&mut self, buf: &[u8]) -> Result<usize>;
    fn flush(&mut self) -> Result<()>;

    /// Write the entire buffer, retrying short writes.
    fn write_all(&mut self, mut buf: &[u8]) -> Result<()> {
        while !buf.is_empty() {
            match self.write(buf) {
                Ok(0) => return Err(Error::from(ErrorKind::WriteZero)),
                Ok(n) => buf = &buf[n..],
                Err(ref e) if e.kind() == ErrorKind::Interrupted => continue,
                Err(e) => return Err(e),
            }
        }
        Ok(())
    }

    /// Write formatted output (the engine behind `write!`).
    fn write_fmt(&mut self, args: fmt::Arguments<'_>) -> Result<()> {
        // Adapter so `core::fmt` can drive a byte sink, capturing the first
        // I/O error.
        struct Adapter<'a, T: ?Sized + 'a> {
            inner: &'a mut T,
            error: Result<()>,
        }
        impl<T: Write + ?Sized> fmt::Write for Adapter<'_, T> {
            fn write_str(&mut self, s: &str) -> fmt::Result {
                match self.inner.write_all(s.as_bytes()) {
                    Ok(()) => Ok(()),
                    Err(e) => {
                        self.error = Err(e);
                        Err(fmt::Error)
                    }
                }
            }
        }
        let mut a = Adapter { inner: self, error: Ok(()) };
        match fmt::write(&mut a, args) {
            Ok(()) => Ok(()),
            Err(_) => {
                if a.error.is_err() {
                    a.error
                } else {
                    Err(Error::from(ErrorKind::Other))
                }
            }
        }
    }
}

/// Enumeration of possible methods to seek within an I/O object. Drop-in for
/// `std::io::SeekFrom`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SeekFrom {
    Start(u64),
    End(i64),
    Current(i64),
}

/// The `std::io::Seek` trait.
pub trait Seek {
    fn seek(&mut self, pos: SeekFrom) -> Result<u64>;
    fn rewind(&mut self) -> Result<()> {
        self.seek(SeekFrom::Start(0))?;
        Ok(())
    }
    fn stream_position(&mut self) -> Result<u64> {
        self.seek(SeekFrom::Current(0))
    }
}

// ---------------------------------------------------------------------------
// Fd: the low-level descriptor handle
// ---------------------------------------------------------------------------

/// A thin handle over a raw file descriptor. Does not close on drop.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Fd(pub i32);

impl Fd {
    pub fn write_all_raw(&self, mut buf: &[u8]) -> core::result::Result<(), Errno> {
        while !buf.is_empty() {
            match syscall::write(self.0, buf) {
                Ok(0) => return Err(Errno(5)),
                Ok(n) => buf = &buf[n..],
                Err(Errno(4)) => continue,
                Err(e) => return Err(e),
            }
        }
        Ok(())
    }
}

impl Read for Fd {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        loop {
            match syscall::read(self.0, buf) {
                Err(Errno(4)) => continue,
                other => return other.map_err(Error::from),
            }
        }
    }
}

impl Write for Fd {
    fn write(&mut self, buf: &[u8]) -> Result<usize> {
        loop {
            match syscall::write(self.0, buf) {
                Err(Errno(4)) => continue,
                other => return other.map_err(Error::from),
            }
        }
    }
    fn flush(&mut self) -> Result<()> {
        Ok(()) // unbuffered
    }
}

impl Seek for Fd {
    fn seek(&mut self, pos: SeekFrom) -> Result<u64> {
        let (off, whence) = match pos {
            SeekFrom::Start(n) => (n as i64, syscall::SEEK_SET),
            SeekFrom::End(n) => (n, syscall::SEEK_END),
            SeekFrom::Current(n) => (n, syscall::SEEK_CUR),
        };
        syscall::lseek(self.0, off, whence).map_err(Error::from)
    }
}

// ---------------------------------------------------------------------------
// Standard streams
// ---------------------------------------------------------------------------

/// Handle to the standard input stream. Drop-in for `std::io::Stdin`.
pub struct Stdin(Fd);
/// Handle to the standard output stream. Drop-in for `std::io::Stdout`.
pub struct Stdout(Fd);
/// Handle to the standard error stream. Drop-in for `std::io::Stderr`.
pub struct Stderr(Fd);

/// Construct a handle to standard input.
pub fn stdin() -> Stdin {
    Stdin(Fd(STDIN))
}
/// Construct a handle to standard output.
pub fn stdout() -> Stdout {
    Stdout(Fd(STDOUT))
}
/// Construct a handle to standard error.
pub fn stderr() -> Stderr {
    Stderr(Fd(STDERR))
}

impl Read for Stdin {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        self.0.read(buf)
    }
}
impl Write for Stdout {
    fn write(&mut self, buf: &[u8]) -> Result<usize> {
        self.0.write(buf)
    }
    fn flush(&mut self) -> Result<()> {
        Ok(())
    }
}
impl Write for Stderr {
    fn write(&mut self, buf: &[u8]) -> Result<usize> {
        self.0.write(buf)
    }
    fn flush(&mut self) -> Result<()> {
        Ok(())
    }
}

// ---- machinery behind the print macros ----

#[doc(hidden)]
pub fn _print(args: fmt::Arguments) {
    let _ = stdout().write_fmt(args);
}

#[doc(hidden)]
pub fn _eprint(args: fmt::Arguments) {
    let _ = stderr().write_fmt(args);
}

// ---------------------------------------------------------------------------
// Write for Vec<u8>, in-memory Cursor, and io::copy
// ---------------------------------------------------------------------------

impl Write for Vec<u8> {
    fn write(&mut self, buf: &[u8]) -> Result<usize> {
        self.extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> Result<()> {
        Ok(())
    }
}

/// Copy the entire contents of `reader` into `writer`. Drop-in for `io::copy`.
pub fn copy<R: Read + ?Sized, W: Write + ?Sized>(reader: &mut R, writer: &mut W) -> Result<u64> {
    let mut buf = [0u8; 8192];
    let mut total = 0u64;
    loop {
        let n = match reader.read(&mut buf) {
            Ok(0) => return Ok(total),
            Ok(n) => n,
            Err(ref e) if e.kind() == ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        };
        writer.write_all(&buf[..n])?;
        total += n as u64;
    }
}

/// An in-memory cursor over a byte buffer. Drop-in for `std::io::Cursor`.
pub struct Cursor<T> {
    inner: T,
    pos: u64,
}

impl<T> Cursor<T> {
    pub fn new(inner: T) -> Cursor<T> {
        Cursor { inner, pos: 0 }
    }
    pub fn position(&self) -> u64 {
        self.pos
    }
    pub fn set_position(&mut self, pos: u64) {
        self.pos = pos;
    }
    pub fn into_inner(self) -> T {
        self.inner
    }
    pub fn get_ref(&self) -> &T {
        &self.inner
    }
    pub fn get_mut(&mut self) -> &mut T {
        &mut self.inner
    }
}

impl<T: AsRef<[u8]>> Read for Cursor<T> {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        let data = self.inner.as_ref();
        let pos = core::cmp::min(self.pos as usize, data.len());
        let n = (&data[pos..]).read(buf)?;
        self.pos += n as u64;
        Ok(n)
    }
}

impl<T: AsRef<[u8]>> Seek for Cursor<T> {
    fn seek(&mut self, pos: SeekFrom) -> Result<u64> {
        let len = self.inner.as_ref().len() as i64;
        let new = match pos {
            SeekFrom::Start(n) => n as i64,
            SeekFrom::End(n) => len + n,
            SeekFrom::Current(n) => self.pos as i64 + n,
        };
        if new < 0 {
            return Err(Error::new(ErrorKind::InvalidInput, "negative seek position"));
        }
        self.pos = new as u64;
        Ok(self.pos)
    }
}

impl Write for Cursor<Vec<u8>> {
    fn write(&mut self, buf: &[u8]) -> Result<usize> {
        let pos = self.pos as usize;
        if pos > self.inner.len() {
            self.inner.resize(pos, 0);
        }
        let end = pos + buf.len();
        if end > self.inner.len() {
            self.inner.resize(end, 0);
        }
        self.inner[pos..end].copy_from_slice(buf);
        self.pos = end as u64;
        Ok(buf.len())
    }
    fn flush(&mut self) -> Result<()> {
        Ok(())
    }
}

// `&[u8]` is a Read source (advances the slice).
impl Read for &[u8] {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        let n = core::cmp::min(buf.len(), self.len());
        buf[..n].copy_from_slice(&self[..n]);
        *self = &self[n..];
        Ok(n)
    }
}

// ---------------------------------------------------------------------------
// BufRead, BufReader, BufWriter
// ---------------------------------------------------------------------------

/// A `Read` that maintains an internal buffer. Drop-in for `std::io::BufRead`.
pub trait BufRead: Read {
    fn fill_buf(&mut self) -> Result<&[u8]>;
    fn consume(&mut self, amt: usize);

    fn read_until(&mut self, delim: u8, buf: &mut Vec<u8>) -> Result<usize> {
        let mut read = 0;
        loop {
            let (done, used) = {
                let available = match self.fill_buf() {
                    Ok(b) => b,
                    Err(ref e) if e.kind() == ErrorKind::Interrupted => continue,
                    Err(e) => return Err(e),
                };
                match available.iter().position(|&b| b == delim) {
                    Some(i) => {
                        buf.extend_from_slice(&available[..=i]);
                        (true, i + 1)
                    }
                    None => {
                        buf.extend_from_slice(available);
                        (false, available.len())
                    }
                }
            };
            self.consume(used);
            read += used;
            if done || used == 0 {
                return Ok(read);
            }
        }
    }

    fn read_line(&mut self, buf: &mut String) -> Result<usize> {
        let mut bytes = Vec::new();
        let n = self.read_until(b'\n', &mut bytes)?;
        let s = core::str::from_utf8(&bytes).map_err(|_| Error::from(ErrorKind::InvalidData))?;
        buf.push_str(s);
        Ok(n)
    }

    fn lines(self) -> Lines<Self>
    where
        Self: Sized,
    {
        Lines { buf: self }
    }
}

/// Iterator over the lines of a `BufRead`. Each line has its terminator removed.
pub struct Lines<B> {
    buf: B,
}
impl<B: BufRead> Iterator for Lines<B> {
    type Item = Result<String>;
    fn next(&mut self) -> Option<Result<String>> {
        let mut line = String::new();
        match self.buf.read_line(&mut line) {
            Ok(0) => None,
            Ok(_) => {
                if line.ends_with('\n') {
                    line.pop();
                    if line.ends_with('\r') {
                        line.pop();
                    }
                }
                Some(Ok(line))
            }
            Err(e) => Some(Err(e)),
        }
    }
}

/// Adds buffering to any reader. Drop-in for `std::io::BufReader`.
pub struct BufReader<R> {
    inner: R,
    buf: Vec<u8>,
    pos: usize,
    cap: usize,
}

impl<R: Read> BufReader<R> {
    pub fn new(inner: R) -> BufReader<R> {
        BufReader::with_capacity(8 * 1024, inner)
    }
    pub fn with_capacity(capacity: usize, inner: R) -> BufReader<R> {
        let mut buf = Vec::with_capacity(capacity);
        buf.resize(capacity, 0);
        BufReader {
            inner,
            buf,
            pos: 0,
            cap: 0,
        }
    }
    pub fn get_ref(&self) -> &R {
        &self.inner
    }
    pub fn get_mut(&mut self) -> &mut R {
        &mut self.inner
    }
    pub fn into_inner(self) -> R {
        self.inner
    }
}

impl<R: Read> Read for BufReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        // Bypass the buffer for large reads when our buffer is empty.
        if self.pos == self.cap && buf.len() >= self.buf.len() {
            return self.inner.read(buf);
        }
        let available = self.fill_buf()?;
        let n = core::cmp::min(available.len(), buf.len());
        buf[..n].copy_from_slice(&available[..n]);
        self.consume(n);
        Ok(n)
    }
}

impl<R: Read> BufRead for BufReader<R> {
    fn fill_buf(&mut self) -> Result<&[u8]> {
        if self.pos >= self.cap {
            self.cap = self.inner.read(&mut self.buf)?;
            self.pos = 0;
        }
        Ok(&self.buf[self.pos..self.cap])
    }
    fn consume(&mut self, amt: usize) {
        self.pos = core::cmp::min(self.pos + amt, self.cap);
    }
}

/// Wraps a writer and buffers its output. Drop-in for `std::io::BufWriter`.
pub struct BufWriter<W: Write> {
    inner: W,
    buf: Vec<u8>,
    capacity: usize,
}

impl<W: Write> BufWriter<W> {
    pub fn new(inner: W) -> BufWriter<W> {
        BufWriter::with_capacity(8 * 1024, inner)
    }
    pub fn with_capacity(capacity: usize, inner: W) -> BufWriter<W> {
        BufWriter {
            inner,
            buf: Vec::with_capacity(capacity),
            capacity,
        }
    }
    pub fn get_ref(&self) -> &W {
        &self.inner
    }
    pub fn get_mut(&mut self) -> &mut W {
        &mut self.inner
    }
    pub fn into_inner(mut self) -> Result<W> {
        self.flush_buf()?;
        // Move out without running Drop's flush again.
        let inner = unsafe { core::ptr::read(&self.inner) };
        let buf = unsafe { core::ptr::read(&self.buf) };
        core::mem::forget(self);
        drop(buf);
        Ok(inner)
    }
    fn flush_buf(&mut self) -> Result<()> {
        if !self.buf.is_empty() {
            self.inner.write_all(&self.buf)?;
            self.buf.clear();
        }
        Ok(())
    }
}

impl<W: Write> Write for BufWriter<W> {
    fn write(&mut self, buf: &[u8]) -> Result<usize> {
        if self.buf.len() + buf.len() > self.capacity {
            self.flush_buf()?;
        }
        if buf.len() >= self.capacity {
            self.inner.write(buf)
        } else {
            self.buf.extend_from_slice(buf);
            Ok(buf.len())
        }
    }
    fn flush(&mut self) -> Result<()> {
        self.flush_buf()?;
        self.inner.flush()
    }
}

impl<W: Write> Drop for BufWriter<W> {
    fn drop(&mut self) {
        let _ = self.flush_buf();
    }
}
