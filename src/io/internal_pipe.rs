// SPDX-License-Identifier: GPL-2.0-or-later

//! Implements an analogue for std::sync::mpsc, except these structs use underlying an underlying Linux
//! pipe which contains a byte if and only if there is at least one message waiting.
//!
//! This uses both an Arc<Mutex<Vec>> and pipes under the hood. In a proper implementation, it should be
//! possible to get rid of the Mutex<Vec> and write the payload to the pipe directly, but that requires
//! some transmutes and I am not knowledgable enough about the Rust memory model to be sure if that is
//! safe, so I take the defensive route and minimize the amount of unsafe stuff done.
//!
//! And if you are wondering: why an Arc<Mutex<Vec>> instead of an mpsc queue? The answer is: because with
//! the Mutex, it is easier to reason about the implementation's correctness (esp. for dealing with buffer
//! overflows); this is not used in critical inner loops so the correctness-for-performance tradeoff is
//! acceptable.

use std::os::unix::io::{RawFd, AsRawFd};
use std::sync::{Arc, Mutex};
use crate::error::SystemError;

pub struct Sender<T> {
    buffer: Arc<Mutex<Vec<T>>>,
    fd: RawFd,
}

impl<T> Sender<T> {
    pub fn send(&self, data: T) -> Result<(), SystemError> {
        let mut guard = self.buffer.lock().map_err(|_| SystemError::new("Internal lock poisoned."))?;
        if guard.is_empty() {
            unsafe { write_byte_to_fd(self.fd)? };
        }
        guard.push(data);
        Ok(())
    }
}

impl<T> AsRawFd for Sender<T> {
    fn as_raw_fd(&self) -> RawFd {
        self.fd
    }
}

impl<T> Drop for Sender<T> {
    fn drop(&mut self) {
        unsafe { libc::close(self.fd) };
    }
}


pub struct Receiver<T> {
    buffer: Arc<Mutex<Vec<T>>>,
    fd: RawFd,
}

impl<T> Receiver<T> {
    pub fn recv_all(&self) -> Result<Vec<T>, SystemError> {
        let mut guard = self.buffer.lock().map_err(|_| SystemError::new("Internal lock poisoned."))?;
        if ! guard.is_empty() {
            unsafe { read_byte_from_fd(self.fd)? };
        }
        
        // Return the entire buffer and put an empty vector in its place.
        Ok(std::mem::take(&mut *guard as &mut Vec<T>))
    }
}

impl<T> AsRawFd for Receiver<T> {
    fn as_raw_fd(&self) -> RawFd {
        self.fd
    }
}

impl<T> Drop for Receiver<T> {
    fn drop(&mut self) {
        unsafe { libc::close(self.fd) };
    }
}


pub fn channel<T>() -> Result<(Sender<T>, Receiver<T>), SystemError> {
    const PIPE_FLAGS: i32 = libc::O_CLOEXEC | libc::O_DIRECT | libc::O_NONBLOCK;

    let mut pipe_fds: [RawFd; 2] = [-1; 2];
    if unsafe { libc::pipe2(&mut pipe_fds as *mut RawFd, PIPE_FLAGS) } < 0 {
        return Err(SystemError::os_with_context("While trying to create internal communication pipes:"));
    };

    // Yes, libc::pipe2() and mpsc::channel() return the read/write handles in the opposide order.
    let [read_fd, write_fd] = pipe_fds;
    let buffer = Arc::new(Mutex::new(Vec::new()));

    Ok((
        Sender {
            buffer: buffer.clone(), fd: write_fd,
        },
        Receiver {
            buffer, fd: read_fd,
        },
    ))
}

const PACKET_SIZE: usize = 1;

unsafe fn write_byte_to_fd(fd: RawFd) -> Result<(), std::io::Error> {
    let buffer: [u8; PACKET_SIZE] = [0; PACKET_SIZE];
    loop {
        let result = libc::write(fd, &buffer as *const _ as *const libc::c_void, PACKET_SIZE);
        if result < 0 {
            let error = std::io::Error::last_os_error();
            match error.kind() {
                std::io::ErrorKind::Interrupted => continue,
                _ => return Err(error),
            }
        }
        break;
    }
    Ok(())
}

unsafe fn read_byte_from_fd(fd: RawFd) -> Result<(), std::io::Error> {
    let mut buffer: [u8; PACKET_SIZE] = [0; PACKET_SIZE];
    loop {
        let result = libc::read(fd, &mut buffer as *mut _ as *mut libc::c_void, PACKET_SIZE);
        if result < 0 {
            let error = std::io::Error::last_os_error();
            match error.kind() {
                std::io::ErrorKind::Interrupted => continue,
                _ => return Err(error),
            }
        }
        break;
    }
    Ok(())
}