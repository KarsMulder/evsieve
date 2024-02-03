// SPDX-License-Identifier: GPL-2.0-or-later

use crate::error::{SystemError, InternalError, RuntimeError};
use std::collections::HashMap;
use std::os::unix::io::{AsRawFd, RawFd};


pub struct Inotify {
    fd: RawFd,
    /// Maps a watch id to a list of all paths that are watched by that id.
    watches: HashMap<i32, Vec<String>>,
}

impl Inotify {
    pub fn new() -> Result<Inotify, SystemError> {
        let fd = unsafe { libc::inotify_init1(libc::IN_NONBLOCK) };
        if fd < 0 {
            return Err(SystemError::os_with_context("While initializing an inotify instance:"));
        }
        Ok(Inotify { fd, watches: HashMap::new() })
    }

    pub fn add_watch(&mut self, path: String) -> Result<(), SystemError> {
        let cstr = match std::ffi::CString::new(path.clone()) {
            Ok(value) => value,
            Err(_) => return Err(SystemError::new("Could not convert a string to a CString."))
        };

        let watch = unsafe {
            libc::inotify_add_watch(
                self.fd,
                cstr.as_ptr(),
                libc::IN_CREATE | libc::IN_MOVED_TO
            )
        };
        if watch < 0 {
            return Err(SystemError::os_with_context(format!(
                "While trying to add \"{}\" to an inotify instance:", path)))
        }
        self.watches.entry(watch).or_default().push(path);
        Ok(())
    }

    pub fn remove_watch(&mut self, path: String) -> Result<(), RuntimeError> {
        // Pre-cache the watch ids so we don't have to borrow self.watches during the loop.
        let watch_ids: Vec<i32> = self.watches.keys().cloned().collect();
        for watch_id in watch_ids {
            let paths = match self.watches.get_mut(&watch_id) {
                Some(paths) => paths,
                None => return Err(InternalError::new("A watch was unexpectedly removed from an Inotify.").into()),
            };
            if paths.contains(&path) {
                paths.retain(|item| item != &path);
                if paths.is_empty() {
                    self.remove_watch_by_id(watch_id)?;
                }
            }
        }

        Ok(())
    }

    fn remove_watch_by_id(&mut self, watch_id: i32) -> Result<(), SystemError> {
        // The error cases should be: self.fd is not valid, watch is not valid.
        // In either case, it is fine that watch is removed from self.watches in case of error.
        let res = unsafe { libc::inotify_rm_watch(self.fd, watch_id) };
        self.watches.remove(&watch_id);

        if res < 0 {
            Err(std::io::Error::last_os_error().into())
        } else {
            Ok(())
        }
    }

    pub fn watched_paths(&self) -> impl Iterator<Item=&String> {
        self.watches.values().flatten()
    }

    /// Adds all watches in the given vector, and removes all not in the given vector.
    pub fn set_watched_paths(&mut self, paths: Vec<String>) -> Result<(), RuntimeError> {
        let paths_to_remove: Vec<String> = self.watched_paths()
            .filter(|&path| !paths.contains(path))
            .cloned().collect();
        for path in paths_to_remove {
            self.remove_watch(path)?;
        }

        let watched_paths: Vec<&String> = self.watched_paths().collect();
        let paths_to_add: Vec<String> = paths.iter()
            .filter(|path| !watched_paths.contains(path))
            .cloned().collect();
        for path in paths_to_add {
            self.add_watch(path)?;
        }
        Ok(())
    }

    /// Does nothing besides clearing out the queued events.
    pub fn poll(&mut self) -> Result<(), SystemError> {
        const NAME_MAX: usize = 255;
        const BUFFER_SIZE: usize = std::mem::size_of::<libc::inotify_event>() + NAME_MAX + 1;
        let mut buffer: [u8; BUFFER_SIZE] = [0; BUFFER_SIZE];
        let res = unsafe {
            libc::read(self.fd, buffer.as_mut_ptr() as *mut libc::c_void, BUFFER_SIZE)
        };

        if res < 0 {
            Err(SystemError::os_with_context("While reading from an inotify instance:"))
        } else {
            Ok(())
        }
    }
}

impl AsRawFd for Inotify {
    fn as_raw_fd(&self) -> RawFd {
        self.fd
    }
}

impl Drop for Inotify {
    fn drop(&mut self) {
        // Ignore any errors because we can't do anything about them.
        unsafe { libc::close(self.fd); }
    }
}