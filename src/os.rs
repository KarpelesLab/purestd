//! `std::os` — OS-specific extensions. We provide the Unix surface: file
//! descriptors (`os::fd` / `os::unix::io`) and a few extension traits.

/// `std::os::fd` — owned and borrowed file descriptors.
pub mod fd {
    use crate::syscall;

    /// A raw file descriptor.
    pub type RawFd = i32;

    /// A trait to extract the raw fd from an underlying object.
    pub trait AsRawFd {
        fn as_raw_fd(&self) -> RawFd;
    }
    /// Construct from a raw fd, taking ownership.
    pub trait FromRawFd {
        /// # Safety
        /// `fd` must be a valid, open descriptor that this object may own.
        unsafe fn from_raw_fd(fd: RawFd) -> Self;
    }
    /// Consume the object and surrender its raw fd (without closing it).
    pub trait IntoRawFd {
        fn into_raw_fd(self) -> RawFd;
    }

    /// An owned file descriptor that closes on drop. Drop-in for
    /// `std::os::fd::OwnedFd`.
    pub struct OwnedFd {
        fd: RawFd,
    }
    impl OwnedFd {
        pub fn try_clone(&self) -> crate::io::Result<OwnedFd> {
            // dup via fcntl(F_DUPFD_CLOEXEC) would be ideal; dup is simplest.
            let new = syscall::dup(self.fd).map_err(crate::io::Error::from)?;
            Ok(OwnedFd { fd: new })
        }
    }
    impl AsRawFd for OwnedFd {
        fn as_raw_fd(&self) -> RawFd {
            self.fd
        }
    }
    impl FromRawFd for OwnedFd {
        unsafe fn from_raw_fd(fd: RawFd) -> OwnedFd {
            OwnedFd { fd }
        }
    }
    impl IntoRawFd for OwnedFd {
        fn into_raw_fd(self) -> RawFd {
            let fd = self.fd;
            core::mem::forget(self);
            fd
        }
    }
    impl Drop for OwnedFd {
        fn drop(&mut self) {
            let _ = syscall::close(self.fd);
        }
    }

    /// A borrowed file descriptor, valid for `'fd`. Drop-in for
    /// `std::os::fd::BorrowedFd`.
    #[derive(Clone, Copy)]
    pub struct BorrowedFd<'fd> {
        fd: RawFd,
        _p: core::marker::PhantomData<&'fd OwnedFd>,
    }
    impl BorrowedFd<'_> {
        /// # Safety
        /// `fd` must remain valid for the lifetime `'fd`.
        pub unsafe fn borrow_raw(fd: RawFd) -> BorrowedFd<'static> {
            BorrowedFd {
                fd,
                _p: core::marker::PhantomData,
            }
        }
    }
    impl AsRawFd for BorrowedFd<'_> {
        fn as_raw_fd(&self) -> RawFd {
            self.fd
        }
    }

    /// Borrow the fd of an object.
    pub trait AsFd {
        fn as_fd(&self) -> BorrowedFd<'_>;
    }
}

/// `std::os::unix`.
pub mod unix {
    /// `std::os::unix::io` — same descriptor types as [`crate::os::fd`].
    pub mod io {
        pub use crate::os::fd::*;
    }

    /// `std::os::unix::fs` — Unix file metadata/permission extensions.
    pub mod fs {
        use crate::fs::{Metadata, Permissions};

        pub trait MetadataExt {
            fn mode(&self) -> u32;
            fn uid(&self) -> u32;
            fn gid(&self) -> u32;
            fn ino(&self) -> u64;
            fn size(&self) -> u64;
        }
        impl MetadataExt for Metadata {
            fn mode(&self) -> u32 { self.raw_mode() }
            fn uid(&self) -> u32 { self.raw_uid() }
            fn gid(&self) -> u32 { self.raw_gid() }
            fn ino(&self) -> u64 { self.raw_ino() }
            fn size(&self) -> u64 { self.len() }
        }

        pub trait PermissionsExt {
            fn mode(&self) -> u32;
            fn from_mode(mode: u32) -> Self;
        }
        impl PermissionsExt for Permissions {
            fn mode(&self) -> u32 { Permissions::mode(self) }
            fn from_mode(mode: u32) -> Permissions { Permissions::from_mode_raw(mode) }
        }
    }

    /// `std::os::unix::ffi` — byte access to `OsStr`/`OsString`.
    pub mod ffi {
        use crate::alloc::string::String;
        use crate::ffi::{OsStr, OsString};

        pub trait OsStrExt {
            fn as_bytes(&self) -> &[u8];
            fn from_bytes(slice: &[u8]) -> &Self;
        }
        impl OsStrExt for OsStr {
            fn as_bytes(&self) -> &[u8] {
                self.to_string_lossy().as_bytes()
            }
            fn from_bytes(slice: &[u8]) -> &OsStr {
                OsStr::new(core::str::from_utf8(slice).unwrap_or(""))
            }
        }

        pub trait OsStringExt {
            fn from_vec(vec: crate::alloc::vec::Vec<u8>) -> Self;
            fn into_vec(self) -> crate::alloc::vec::Vec<u8>;
        }
        impl OsStringExt for OsString {
            fn from_vec(vec: crate::alloc::vec::Vec<u8>) -> OsString {
                OsString::from(String::from_utf8_lossy(&vec).into_owned())
            }
            fn into_vec(self) -> crate::alloc::vec::Vec<u8> {
                self.into_string().unwrap_or_default().into_bytes()
            }
        }
    }

    /// `std::os::unix::prelude`.
    pub mod prelude {
        pub use super::ffi::{OsStrExt, OsStringExt};
        pub use super::fs::{MetadataExt, PermissionsExt};
        pub use super::io::*;
    }
}
