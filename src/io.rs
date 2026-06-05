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

/// Map a raw errno to an [`ErrorKind`] (values shared by Linux & Darwin for the
/// common cases).
fn errno_kind(c: i32) -> ErrorKind {
    use ErrorKind::*;
    match c {
        1 => PermissionDenied,   // EPERM
        2 => NotFound,           // ENOENT
        4 => Interrupted,        // EINTR
        13 => PermissionDenied,  // EACCES
        17 => AlreadyExists,     // EEXIST
        22 => InvalidInput,      // EINVAL
        32 => BrokenPipe,        // EPIPE
        35 | 11 => WouldBlock,   // EAGAIN/EWOULDBLOCK (Darwin 35, Linux 11)
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
