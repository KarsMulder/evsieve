// SPDX-License-Identifier: GPL-2.0-or-later

//! Implements an analogue for std::sync::mpsc, except these structs use underlying an underlying Linux
//! pipe, so they can be polled using the standard POSIX API's.
//!
//! Do not use for communication with other processes or even subprocesses.

use std::marker::PhantomData;
use std::mem::MaybeUninit;
use std::os::unix::io::{RawFd, AsRawFd};
use crate::error::SystemError;

pub struct Sender<T: 'static> {
    fd: RawFd,
    _phantom: PhantomData<T>,
}

impl<T: 'static> Sender<T> {
    pub fn send(&self, data: T) -> Result<(), SystemError> {
        // We use MaybeUninit (and not ManuallyDrop) to signal the compiler that the data should not
        // be considered "valid" anymore after it has been sent to the kernel, so we avoid violating
        // some aliasing rules.
        let data_size: usize = std::mem::size_of::<T>();
        assert!(data_size <= libc::PIPE_BUF);
        let data = MaybeUninit::new(data);

        loop {
            let result = unsafe { libc::write(
                self.fd, &data as *const _ as *const libc::c_void, data_size
            )};
            if result < 0 {
                let error = std::io::Error::last_os_error();
                match error.kind() {
                    std::io::ErrorKind::Interrupted => continue,
                    _ => return Err(error.into()),
                }
            } else if result == data_size as isize {
                // Data successfully written.
                return Ok(());
            } else {
                // A packet was partially written. This should not be possible given O_DIRECT was set.
                return Err(SystemError::new("Partial write made to internal pipe."));
            }
        }
    }
}

impl<T: 'static> AsRawFd for Sender<T> {
    fn as_raw_fd(&self) -> RawFd {
        self.fd
    }
}

impl<T: 'static> Drop for Sender<T> {
    fn drop(&mut self) {
        unsafe { libc::close(self.fd) };
    }
}


pub struct Receiver<T: 'static> {
    fd: RawFd,
    _phantom: PhantomData<T>,
}

impl<T: 'static> Receiver<T> {
    pub fn recv(&self) -> Result<T, SystemError> {
        let data_size = std::mem::size_of::<T>();
        assert!(data_size <= libc::PIPE_BUF);
        let mut data: MaybeUninit<T> = MaybeUninit::uninit();

        loop {
            let result = unsafe { libc::read(
                self.fd, &mut data as *mut _ as *mut libc::c_void, data_size
            )};
            if result < 0 {
                let error = std::io::Error::last_os_error();
                match error.kind() {
                    std::io::ErrorKind::Interrupted => continue,
                    _ => return Err(error.into()),
                }
            } else if result == data_size as isize {
                // Data successfully read.
                return Ok(unsafe { data.assume_init() });
            } else {
                // A packet was partially read. This should not be possible given O_DIRECT was set.
                return Err(SystemError::new("Partial write made to internal pipe."));
            }
        }
    }
}

impl<T: 'static> AsRawFd for Receiver<T> {
    fn as_raw_fd(&self) -> RawFd {
        self.fd
    }
}

impl<T: 'static> Drop for Receiver<T> {
    fn drop(&mut self) {
        unsafe { libc::close(self.fd) };
    }
}


pub fn channel<T: 'static>() -> Result<(Sender<T>, Receiver<T>), SystemError> {
    assert!(std::mem::size_of::<T>() <= libc::PIPE_BUF);
    const PIPE_FLAGS: i32 = libc::O_CLOEXEC | libc::O_DIRECT | libc::O_NONBLOCK;

    let mut pipe_fds: [RawFd; 2] = [-1; 2];
    if unsafe { libc::pipe2(&mut pipe_fds as *mut _ as *mut RawFd, PIPE_FLAGS) } < 0 {
        return Err(SystemError::os_with_context("While trying to create internal communication pipes:"));
    };

    let [read_fd, write_fd] = pipe_fds;

    Ok((
        Sender { fd: write_fd, _phantom: PhantomData },
        Receiver { fd: read_fd, _phantom: PhantomData },
    ))
}
