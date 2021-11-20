// SPDX-License-Identifier: GPL-2.0-or-later

use crate::error::{SystemError};
use crate::io::fd::{HasFixedFd};
use std::collections::HashMap;


/// Like a file descriptor, that identifies a file registered in this Epoll.
#[derive(Clone, Copy, Hash, PartialEq, Eq)]
pub struct FileIndex(u64);

/// The epoll is responsible for detecting which input devices have events available.
/// The evsieve program spends most of its time waiting on Epoll::poll, which waits until
/// some input device has events available.
/// 
/// It also keeps track of when input devices unexpectedly close.
pub struct Epoll<T: HasFixedFd> {
    files: HashMap<FileIndex, T>,
    /// A counter, so every file registered can get an unique index in the files map.
    counter: u64,
}

/// Represents a result that an Epoll may return.
pub enum Message {
    Ready(FileIndex),
    Broken(FileIndex),
}

impl<T: HasFixedFd> Epoll<T> {
    pub fn new() -> Result<Epoll<T>, SystemError> {
        Ok(Epoll {
            files: HashMap::new(),
            counter: 0,
        })
    }

    fn get_unique_index(&mut self) -> FileIndex {
        self.counter += 1;
        FileIndex(self.counter)
    }

    /// # Safety
    /// The file must return a valid raw file descriptor.
    pub unsafe fn add_file(&mut self, file: T) -> Result<FileIndex, SystemError> {
        let index = self.get_unique_index();
        let file_fd = file.as_raw_fd();

        // Sanity check: make sure we don't add a file that already belongs to this epoll.
        if self.files.values().any(|opened_file| opened_file.as_raw_fd() == file_fd) {
            return Err(SystemError::new("Cannot add a file to an epoll that already belongs to said epoll."));
        }
        self.files.insert(index, file);
        Ok(index) // TODO: this function is infallible.
    }

    /// Returns an iterator over all files belonging to this epoll.
    pub fn files(&self) -> impl Iterator<Item=&T> {
        self.files.values()
    }

    pub fn contains_index(&self, index: FileIndex) -> bool {
        self.files.contains_key(&index)
    }

    pub fn get(&self, index: FileIndex) -> Option<&T> {
        self.files.get(&index)
    }

    pub fn get_mut(&mut self, index: FileIndex) -> Option<&mut T> {
        self.files.get_mut(&index)
    }

    /// Removes a file specified by an index from this epoll.
    pub fn remove(&mut self, index: FileIndex) -> Option<T> {
        let file = match self.files.remove(&index) {
            Some(file) => file,
            None => return None,
        };

        Some(file)
    }

    fn poll_raw(&mut self) -> Result<Vec<(FileIndex, i16)>, std::io::Error> {
        let mut poll_fds: Vec<libc::pollfd> = self.files().map(|file| {
            libc::pollfd {
                fd: file.as_raw_fd(),
                events: libc::POLLIN,
                revents: 0,
            }
        }).collect();

        let result = unsafe {
            libc::poll(
                poll_fds.as_mut_ptr(),
                poll_fds.len() as libc::nfds_t,
                -1, // Timeout, -1 means indefinitely.
            )
        };

        if result < 0 {
            Err(std::io::Error::last_os_error())
        } else {
            // TODO: Refactor this monstrosity.
            Ok(self.files.iter().zip(poll_fds.into_iter()).map(|((index, _), poll_fd)| {
                (*index, poll_fd.revents)
            }).collect())
        }
    }

    /// Tries to read all events from all ready devices. Returns a vector containing all events read.
    /// If a device reports an error, said device is removed from self and also returned.
    pub fn poll(&mut self) -> Result<impl Iterator<Item=Message>, SystemError> {
        let events = loop {
            match self.poll_raw() {
                Ok(events) => break events,
                Err(error) => match error.kind() {
                    std::io::ErrorKind::Interrupted => continue,
                    _ => return Err(error.into())
                }
            }
        };

        // Create a list of which devices are ready and which are broken.
        let mut messages: Vec<Message> = Vec::new();

        for event in events {
            let file_index: FileIndex = event.0;

            if event.1 & libc::POLLIN != 0 {
                messages.push(Message::Ready(file_index));
            }
            if event.1 & libc::POLLERR != 0 || event.1 & libc::POLLHUP != 0 {
                messages.push(Message::Broken(file_index));
            }
        }

        Ok(messages.into_iter())
    }

    /// Returns whether currently any files are opened under this epoll.
    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }
}

impl<T: HasFixedFd> std::ops::Index<FileIndex> for Epoll<T> {
    type Output = T;
    fn index(&self, index: FileIndex) -> &Self::Output {
        &self.files[&index]
    }
}

impl<T: HasFixedFd> std::ops::IndexMut<FileIndex> for Epoll<T> {
    fn index_mut(&mut self, index: FileIndex) -> &mut Self::Output {
        self.files.get_mut(&index).expect("Internal error: attempt to retrieve a file that does not belong to this epoll.")
    }
}
