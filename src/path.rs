//! `std::path` subset. `Path`/`PathBuf` are thin UTF-8 wrappers (real `std`
//! keeps raw OS bytes); enough for opening files and basic manipulation.

use crate::alloc::string::String;
use core::fmt;
use core::ops::Deref;

const SEP: char = '/';

/// A borrowed path. Drop-in-ish for `std::path::Path`.
#[repr(transparent)]
pub struct Path(str);

impl Path {
    pub fn new<S: AsRef<str> + ?Sized>(s: &S) -> &Path {
        unsafe { &*(s.as_ref() as *const str as *const Path) }
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
    pub fn to_str(&self) -> Option<&str> {
        Some(&self.0)
    }
    pub fn is_absolute(&self) -> bool {
        self.0.starts_with(SEP)
    }
    pub fn is_relative(&self) -> bool {
        !self.is_absolute()
    }
    pub fn file_name(&self) -> Option<&str> {
        if self.0.is_empty() || self.0.ends_with(SEP) {
            return None;
        }
        match self.0.rfind(SEP) {
            Some(i) => Some(&self.0[i + 1..]),
            None => Some(&self.0),
        }
    }
    pub fn parent(&self) -> Option<&Path> {
        let trimmed = self.0.trim_end_matches(SEP);
        match trimmed.rfind(SEP) {
            Some(0) => Some(Path::new("/")),
            Some(i) => Some(Path::new(&trimmed[..i])),
            None => None,
        }
    }
    pub fn extension(&self) -> Option<&str> {
        let name = self.file_name()?;
        match name.rfind('.') {
            Some(0) | None => None,
            Some(i) => Some(&name[i + 1..]),
        }
    }
    pub fn join<P: AsRef<Path>>(&self, p: P) -> PathBuf {
        let mut out = PathBuf::from(&self.0);
        out.push(p);
        out
    }
    pub fn to_path_buf(&self) -> PathBuf {
        PathBuf::from(&self.0)
    }
}

impl fmt::Debug for Path {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self.0, f)
    }
}
impl fmt::Display for Path {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
impl AsRef<Path> for Path {
    fn as_ref(&self) -> &Path {
        self
    }
}
impl AsRef<Path> for str {
    fn as_ref(&self) -> &Path {
        Path::new(self)
    }
}
impl AsRef<Path> for String {
    fn as_ref(&self) -> &Path {
        Path::new(self.as_str())
    }
}

/// An owned, mutable path. Drop-in-ish for `std::path::PathBuf`.
#[derive(Clone, Default, PartialEq, Eq)]
pub struct PathBuf(String);

impl PathBuf {
    pub fn new() -> PathBuf {
        PathBuf(String::new())
    }
    pub fn as_path(&self) -> &Path {
        Path::new(self.0.as_str())
    }
    pub fn push<P: AsRef<Path>>(&mut self, p: P) {
        let p = &p.as_ref().0;
        if p.starts_with(SEP) {
            self.0.clear();
            self.0.push_str(p);
            return;
        }
        if !self.0.is_empty() && !self.0.ends_with(SEP) {
            self.0.push(SEP);
        }
        self.0.push_str(p);
    }
    pub fn into_string(self) -> String {
        self.0
    }
}

impl From<&str> for PathBuf {
    fn from(s: &str) -> PathBuf {
        PathBuf(String::from(s))
    }
}
impl From<&String> for PathBuf {
    fn from(s: &String) -> PathBuf {
        PathBuf(s.clone())
    }
}
impl From<String> for PathBuf {
    fn from(s: String) -> PathBuf {
        PathBuf(s)
    }
}
impl Deref for PathBuf {
    type Target = Path;
    fn deref(&self) -> &Path {
        self.as_path()
    }
}
impl AsRef<Path> for PathBuf {
    fn as_ref(&self) -> &Path {
        self.as_path()
    }
}
impl fmt::Debug for PathBuf {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self.0, f)
    }
}
impl fmt::Display for PathBuf {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
