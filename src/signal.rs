// SPDX-License-Identifier: GPL-2.0-or-later

use std::mem::MaybeUninit;
use std::os::unix::prelude::{AsRawFd, RawFd};

use crate::error::{SystemError, Context};
use crate::io::fd::{HasFixedFd, OwnedFd};

/// As long as a SignalBlock exists, this program will not receive any signals unless it asks
/// for them. Only one SignalBlock should ever exist simultaneously, having more of them is
/// a logical error that can permanently destroy the program's ability to receive signals.
pub struct SignalBlock {
    orig_sigmask: libc::sigset_t,
}

#[repr(transparent)]
pub struct SigMask(libc::sigset_t);

#[allow(clippy::should_implement_trait)]
impl SigMask {
    pub fn new() -> SigMask {
        unsafe {
            let mut sigmask: std::mem::MaybeUninit<libc::sigset_t> = MaybeUninit::zeroed();
            libc::sigemptyset(sigmask.as_mut_ptr());
            SigMask(sigmask.assume_init())
        }
    }

    pub fn as_mut(&mut self) -> &mut libc::sigset_t {
        &mut self.0
    }

    pub fn as_ref(&self) -> &libc::sigset_t {
        &self.0
    }

    pub fn fill(&mut self) -> &mut Self {
        unsafe { libc::sigfillset(self.as_mut()); }
        self
    }

    pub fn add(&mut self, signal: libc::c_int) -> &mut Self {
        unsafe { libc::sigaddset(self.as_mut(), signal); }
        self
    }

    pub fn del(&mut self, signal: libc::c_int) -> &mut Self {
        unsafe { libc::sigdelset(self.as_mut(), signal); }
        self
    }
}

impl SignalBlock {
    /// # Safety
    /// Only one SignalBlock should exist at any time.
    pub unsafe fn new(mask: &SigMask) -> Result<SignalBlock, std::io::Error> {
        let mut orig_sigmask: MaybeUninit<libc::sigset_t> = MaybeUninit::zeroed();
        let res = libc::sigprocmask(libc::SIG_SETMASK, mask.as_ref(), orig_sigmask.as_mut_ptr());
        if res < 0 {
            Err(std::io::Error::last_os_error())
        } else {
            Ok(SignalBlock { orig_sigmask: orig_sigmask.assume_init() })
        }
    }

    pub fn orig_sigmask(&self) -> &libc::sigset_t {
        &self.orig_sigmask
    }
}

impl Drop for SignalBlock {
    fn drop(&mut self) {
        unsafe {
            let orig_sigmask_ptr = &self.orig_sigmask as *const libc::sigset_t;
            libc::sigprocmask(libc::SIG_SETMASK, orig_sigmask_ptr, std::ptr::null_mut());
        }
    }
}

pub type SignalNumber = libc::c_int;

pub struct SignalFd {
    /// The signal fd to communicate with the OS. Beware: SignalFd implements HasFixedFd.
    fd: OwnedFd,
}

impl SignalFd {
    pub fn new(sigmask: &SigMask) -> Result<SignalFd, SystemError> {
        let fd: OwnedFd = unsafe {
            OwnedFd::from_syscall(
                libc::signalfd(-1, sigmask.as_ref(), libc::SFD_NONBLOCK | libc::SFD_CLOEXEC)
            ).with_context("While creating a signal fd:")?
        };
        Ok(SignalFd { fd })
    }

    pub fn read_raw(&mut self) -> Result<libc::signalfd_siginfo, std::io::Error> {
        const SIGNAL_INFO_SIZE: usize = std::mem::size_of::<libc::signalfd_siginfo>();
        let mut signal_info: MaybeUninit<libc::signalfd_siginfo> = MaybeUninit::uninit();
        let result = unsafe { libc::read(self.as_raw_fd(), signal_info.as_mut_ptr() as *mut libc::c_void, SIGNAL_INFO_SIZE) };
        
        if result == SIGNAL_INFO_SIZE as isize {
            Ok(unsafe { signal_info.assume_init() })
        } else if result < 0 {
            Err(std::io::Error::last_os_error())
        } else if result == 0 {
            Err(std::io::Error::new(std::io::ErrorKind::WouldBlock, "Read zero bytes from a signalfd."))
        } else {
            Err(std::io::Error::new(std::io::ErrorKind::Other, "Reading a signalfd returned invalid amount of bytes."))
        }
    }
}

impl AsRawFd for SignalFd {
    fn as_raw_fd(&self) -> RawFd {
        self.fd.as_raw_fd()
    }
}
unsafe impl HasFixedFd for SignalFd {}