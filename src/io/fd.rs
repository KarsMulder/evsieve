// SPDX-License-Identifier: GPL-2.0-or-later

use crate::error::SystemError;
use std::os::unix::io::{FromRawFd, AsRawFd, RawFd};

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

impl AsRawFd for OwnedFd {
    fn as_raw_fd(&self) -> RawFd {
        self.0
    }
}

impl Drop for OwnedFd {
    fn drop(&mut self) {
        unsafe { libc::close(self.as_raw_fd()) };
    }
}

/// An unsafe marker trait: if a structure implements this trait, it promises that its file descriptor
/// will cannot be changed by functions that do not own the structure, i.e. no function that takes a
/// (mutable) reference is allowed to modify the structure in a way that makes as_raw_fd() return a
/// different value.
///
/// Furthermore, the result returned by AsRawFd must be guaranteed to be valid for the duration of the
/// structure.
///
/// Changing the file descriptor of a struct with this trait through a reference may invoke undefined
/// behaviour. Unsafe code may assume that the file descriptor does not change even if it hands out an
/// &mut reference to a structure with HasFixedFd.
///
/// This constraint is unfortunately unsound, because even if in a given module there is no code that
/// allows changing a file descriptor through &mut, it is always possible to construct a second instance
/// of a certain struct and then std::men::swap() them. This could happen anywhere in safe code.
///
/// I really don't like this current approach and of course this attitude towards unsafety
/// would be unacceptable in a library, but I don't see a way around it other than (1) moving away from
/// epoll() towards poll(), possibly introducing a performance regression, (2) decoupling the file
/// descriptors from the surrounding data, which increases code complexity and probably introduces a
/// lot more potential for unsafety, (3) adding additional verification code to the `Epoll`class,
/// which comes at a performance penalty.
///
/// Maybe one day I'll start using poll() if benchmarks show that it has no measurable performance
/// impact. Other than that, I think that putting up with this trait is just the least of the many
/// possible evils.
///
/// To be clear: just because a certain structure X implements this trait, does not mean that any
/// structure containing X has that trait as well. For example, OwnedFd implements it because there
/// is no (safe) function that modifies OwnedFd in a way that changes its file descriptor, but any
/// struct containing OwnedFd still needs to implement it to guarantee that it will not swap out its
/// OwnedFd for another OwnedFd.
pub unsafe trait HasFixedFd : AsRawFd {}

unsafe impl HasFixedFd for OwnedFd {}