//! `std::ffi` subset.
//!
//! `CStr`/`CString` are re-exported from `core`/`alloc`. `OsStr`/`OsString` are
//! modeled as UTF-8 (a simplification versus real `std`, which keeps raw bytes);
//! this is sufficient for the path/env surface here.

pub use crate::alloc::ffi::CString;
pub use core::ffi::CStr;

use crate::alloc::string::String;
use core::fmt;
use core::ops::Deref;

/// Borrowed OS string. Here, a transparent wrapper over `str`.
#[repr(transparent)]
pub struct OsStr(str);

impl OsStr {
    pub fn new<S: AsRef<str> + ?Sized>(s: &S) -> &OsStr {
        // Safe: OsStr is repr(transparent) over str.
        unsafe { &*(s.as_ref() as *const str as *const OsStr) }
    }
    pub fn to_str(&self) -> Option<&str> {
        Some(&self.0)
    }
    pub fn to_string_lossy(&self) -> &str {
        &self.0
    }
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl fmt::Debug for OsStr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self.0, f)
    }
}
impl fmt::Display for OsStr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
impl AsRef<OsStr> for OsStr {
    fn as_ref(&self) -> &OsStr {
        self
    }
}
impl AsRef<OsStr> for str {
    fn as_ref(&self) -> &OsStr {
        OsStr::new(self)
    }
}
impl AsRef<OsStr> for String {
    fn as_ref(&self) -> &OsStr {
        OsStr::new(self.as_str())
    }
}

/// Owned OS string. Here, a wrapper over `String`.
#[derive(Clone, Default, PartialEq, Eq)]
pub struct OsString(String);

impl OsString {
    pub fn new() -> OsString {
        OsString(String::new())
    }
    pub fn from_string(s: String) -> OsString {
        OsString(s)
    }
    pub fn into_string(self) -> core::result::Result<String, OsString> {
        Ok(self.0)
    }
    pub fn to_str(&self) -> Option<&str> {
        Some(&self.0)
    }
}

impl Deref for OsString {
    type Target = OsStr;
    fn deref(&self) -> &OsStr {
        OsStr::new(self.0.as_str())
    }
}
impl From<String> for OsString {
    fn from(s: String) -> OsString {
        OsString(s)
    }
}
impl From<&str> for OsString {
    fn from(s: &str) -> OsString {
        OsString(String::from(s))
    }
}
impl fmt::Debug for OsString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self.0, f)
    }
}
impl fmt::Display for OsString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
