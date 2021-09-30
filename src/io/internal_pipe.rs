// SPDX-License-Identifier: GPL-2.0-or-later

//! Implements an analogue for std::sync::mpsc, except these structs use underlying an underlying Linux
//! pipe which is nonempty if and only if there is at least one message waiting.
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
        unsafe { write_byte_to_fd(self.fd)? };
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
        unsafe { read_bytes_until_empty(self.fd)? };
        
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
    const PIPE_FLAGS: i32 = libc::O_CLOEXEC | libc::O_NONBLOCK;

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

unsafe fn write_byte_to_fd(fd: RawFd) -> Result<(), std::io::Error> {
    const PACKET_SIZE: usize = 1;
    type Buffer = [u8; PACKET_SIZE];
    let buffer: Buffer = [0; PACKET_SIZE];

    loop {
        let result = libc::write(fd, &buffer as *const _ as *const libc::c_void, std::mem::size_of::<Buffer>());
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

#[allow(clippy::comparison_chain)]
unsafe fn read_bytes_until_empty(fd: RawFd) -> Result<(), std::io::Error> {
    const BUFFER_SIZE: usize = 32;
    type Buffer = [u8; BUFFER_SIZE];
    let mut buffer: Buffer = [0; BUFFER_SIZE];

    loop {
        let result = libc::read(fd, &mut buffer as *mut _ as *mut libc::c_void, std::mem::size_of::<Buffer>());
        
        if result > 0 {
            // Some bytes were successfully read. There might be more bytes to read.
            continue;
        } else if result < 0 {
            // An error occurred. The pipe may be empty (WouldBlock) or worse.
            let error = std::io::Error::last_os_error();
            match error.kind() {
                std::io::ErrorKind::Interrupted => continue,
                std::io::ErrorKind::WouldBlock  => break,
                _ => return Err(error),
            }
        } else {
            // What is going on if result == 0 is not specified by the POSIX standard. Let's just break
            // the loop and hope nothing bad happens.
            break;
        }
    }
    Ok(())
}