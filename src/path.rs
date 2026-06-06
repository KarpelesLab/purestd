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

// ---------------------------------------------------------------------------
// More Path / PathBuf methods
// ---------------------------------------------------------------------------

impl Path {
    /// The portion of `file_name` before the last `.` (whole name if none).
    pub fn file_stem(&self) -> Option<&str> {
        let name = self.file_name()?;
        match name.rfind('.') {
            Some(0) | None => Some(name),
            Some(i) => Some(&name[..i]),
        }
    }

    pub fn starts_with<P: AsRef<Path>>(&self, base: P) -> bool {
        let base = base.as_ref().as_str().trim_end_matches('/');
        let me = self.0.trim_end_matches('/');
        if base.is_empty() {
            return true;
        }
        me == base || me.strip_prefix(base).map(|r| r.starts_with('/')).unwrap_or(false)
    }
    pub fn ends_with<P: AsRef<Path>>(&self, child: P) -> bool {
        let child = child.as_ref().as_str().trim_matches('/');
        let me = self.0.trim_end_matches('/');
        me == child
            || me.strip_suffix(child).map(|r| r.ends_with('/')).unwrap_or(false)
    }
    pub fn strip_prefix<P: AsRef<Path>>(&self, base: P) -> Result<&Path, StripPrefixError> {
        let base = base.as_ref().as_str().trim_end_matches('/');
        if base.is_empty() {
            return Ok(self);
        }
        if let Some(rest) = self.0.strip_prefix(base) {
            let rest = rest.trim_start_matches('/');
            return Ok(Path::new(rest));
        }
        Err(StripPrefixError(()))
    }

    pub fn with_extension<S: AsRef<str>>(&self, ext: &S) -> PathBuf {
        let mut buf = self.to_path_buf();
        buf.set_extension(ext);
        buf
    }
    pub fn with_file_name<S: AsRef<str>>(&self, name: &S) -> PathBuf {
        match self.parent() {
            Some(p) => p.join(Path::new(name.as_ref())),
            None => PathBuf::from(name.as_ref()),
        }
    }

    /// A `Display` adapter (paths are UTF-8 here, so this never lossily escapes).
    pub fn display(&self) -> Display<'_> {
        Display { path: self }
    }

    pub fn exists(&self) -> bool {
        crate::fs::metadata(self).is_ok()
    }
    pub fn is_file(&self) -> bool {
        crate::fs::metadata(self).map(|m| m.is_file()).unwrap_or(false)
    }
    pub fn is_dir(&self) -> bool {
        crate::fs::metadata(self).map(|m| m.is_dir()).unwrap_or(false)
    }
    pub fn metadata(&self) -> crate::io::Result<crate::fs::Metadata> {
        crate::fs::metadata(self)
    }
    pub fn read_dir(&self) -> crate::io::Result<crate::fs::ReadDir> {
        crate::fs::read_dir(self)
    }
}

/// Returned by [`Path::strip_prefix`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StripPrefixError(());
impl fmt::Display for StripPrefixError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("prefix not found")
    }
}
impl core::error::Error for StripPrefixError {}

/// `Display` adapter from [`Path::display`].
pub struct Display<'a> {
    path: &'a Path,
}
impl fmt::Display for Display<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.path.0)
    }
}

impl PathBuf {
    /// Truncate to the parent, returning false if there was nothing to pop.
    pub fn pop(&mut self) -> bool {
        match self.as_path().parent() {
            Some(p) => {
                let p = String::from(p.as_str());
                self.0 = p;
                true
            }
            None => false,
        }
    }
    pub fn set_file_name<S: AsRef<str>>(&mut self, name: S) {
        if self.as_path().file_name().is_some() {
            self.pop();
        }
        self.push(Path::new(name.as_ref()));
    }
    pub fn set_extension<S: AsRef<str>>(&mut self, ext: S) -> bool {
        let ext = ext.as_ref();
        let stem = match self.as_path().file_stem() {
            Some(s) => String::from(s),
            None => return false,
        };
        let mut name = stem;
        if !ext.is_empty() {
            name.push('.');
            name.push_str(ext);
        }
        self.set_file_name(name);
        true
    }
}
