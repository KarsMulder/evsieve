// SPDX-License-Identifier: GPL-2.0-or-later

//! Implements an analogue for std::sync::mpsc, except these structs use underlying Linux pipes with
//! file descriptors, so they can be polled on by Epoll.
//!
//! This uses both mpsc queues and pipes under the hood. In a proper implementation, it should be possible
//! to get rid of the mpsc queues, but that requires some transmutes and I am not knowledgable enough
//! about the Rust memory model to be sure if that is safe, so I take the defensive route and minimize
//! the amount of unsafe stuff done.

use std::os::unix::io::{RawFd, AsRawFd};
use std::sync::mpsc::{self, TryRecvError};
use crate::error::SystemError;

const PACKET_SIZE: usize = 1;

pub struct Sender<T> {
    sender: mpsc::Sender<T>,
    fd: RawFd,
}

impl<T> Sender<T> {
    pub fn send(&self, data: T) -> Result<(), SystemError> {
        self.sender.send(data).map_err(|_| SystemError::new("Internal communication channel broken."))?;
        let buffer: [u8; PACKET_SIZE] = [0; PACKET_SIZE];
        let result = unsafe { libc::write(self.fd, &buffer as *const _ as *const libc::c_void, PACKET_SIZE)};
        if result < 0 {
            Err(SystemError::os_with_context("Internal communication channel broken:"))
        } else {
            Ok(())
        }
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
    receiver: mpsc::Receiver<T>,
    fd: RawFd,
}

impl<T> Receiver<T> {
    pub fn recv(&self) -> Result<T, TryRecvError> {
        let mut buffer: [u8; PACKET_SIZE] = [0; PACKET_SIZE];
        let result = unsafe { libc::read(self.fd, &mut buffer as *mut _ as *mut libc::c_void, PACKET_SIZE)};
        if result < 0 {
            Err(TryRecvError::Empty)
        } else {
            self.receiver.try_recv()
        }
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
    let (sender, receiver) = mpsc::channel();

    Ok((
        Sender {
            sender, fd: write_fd,
        },
        Receiver {
            receiver, fd: read_fd,
        },
    ))
}