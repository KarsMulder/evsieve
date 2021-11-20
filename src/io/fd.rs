use crate::error::SystemError;
use std::os::unix::io::FromRawFd;
pub use std::os::unix::io::RawFd;

/// A wrapper around a file descriptor that calls `libc::close` on the descriptor when it is dropped.
/// Guarantees that the file descriptor it owns is valid for the lifetime of this structure.
#[repr(transparent)]
pub struct OwnedFd(RawFd);

impl OwnedFd {
    /// Takes ownership of a given file descriptor.
    /// 
    /// # Safety
    /// The file descriptor must be valid. Furthermore, it must not be closed by anything else during
    /// the lifetime of this struct.
    /// 
    /// # Panics
    /// Panics if the passed fd is below zero.
    pub unsafe fn new(fd: RawFd) -> OwnedFd {
        OwnedFd::from_raw_fd(fd)
    }

    /// To be called on the result of a syscall that returns a file descriptor. Takes ownership of
    /// the given file descriptor if positive, otherwise returns the last OS error.
    ///
    /// # Safety
    /// The file descriptor must be valid or negative. Furthermore, it must not be closed by anything
    /// else during the lifetime of this struct.
    pub unsafe fn from_syscall(fd: libc::c_int) -> Result<OwnedFd, SystemError> {
        if fd >= 0 {
            Ok(OwnedFd::new(fd))
        } else {
            Err(std::io::Error::last_os_error().into())
        }
    }
}

impl FromRawFd for OwnedFd {
    unsafe fn from_raw_fd(fd: RawFd) -> Self {
        if fd < 0 {
            panic!("A file descriptor below zero was encountered. This suggests an unhandled I/O error.");
        }
        OwnedFd(fd)
    }
}

// I would split this trait into two: AsOwnedFd and AsRawFd, but that runs afoul of Rust's orphan rules
// because `impl AsRawFd for T where T: AsOwnedFd` does not guarantee that T is a local type.
pub trait AsFd {
    fn as_owned_fd(&self) -> &OwnedFd;

    fn as_raw_fd(&self) -> RawFd {
        // Has a specialised implementation for OwnedFd.
        self.as_owned_fd().as_raw_fd()
    }
}

impl AsFd for OwnedFd {
    fn as_owned_fd(&self) -> &OwnedFd {
        self
    }

    fn as_raw_fd(&self) -> RawFd {
        self.0
    }
}

impl Drop for OwnedFd {
    fn drop(&mut self) {
        unsafe { libc::close(self.as_raw_fd()) };
    }
}