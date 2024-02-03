// SPDX-License-Identifier: GPL-2.0-or-later

use crate::error::{Context, RuntimeError, SystemError};
use crate::utils::NonCopy;
use std::collections::HashMap;
use std::os::unix::io::{AsRawFd, RawFd};

type WatchId = NonCopy<i32>;

pub struct Inotify {
    fd: RawFd,
    /// Maps a watch id to a list of all paths that are watched by that id.
    watches: HashMap<NonCopy<i32>, Vec<String>>,
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
        let watch = WatchId::new(watch);
        if cfg!(feature = "debug-persistence") {
            println!("Adding watch to \"{path}\". It has been assigned the id of {watch}. Under that ID, the following paths were already registered: {:?}", self.watches.get(&watch));
        }

        self.watches.entry(watch).or_default().push(path);

        if cfg!(feature = "debug-persistence") {
            println!("There are watches registered under the following IDs: {}", format_list_of_active_ids(&self.watches));
        }
        Ok(())
    }

    pub fn remove_watch(&mut self, path: String) {
        // Pre-cache the watch ids so we don't have to borrow self.watches during the loop.
        for (_id, paths) in self.watches.iter_mut() {
            paths.retain(|item| item != &path);
        }

        if cfg!(feature = "debug-persistence") {
            println!("Removing watch to \"{path}\".");
        }

        // This could be done nicely with the experimental `HashMap::extract_if` function.
        // But it's not stable yet, so it'll have to happen the ugly way.
        let mut retained_watches = HashMap::new();
        for (watch_id, paths) in self.watches.drain() {
            if ! paths.is_empty() {
                retained_watches.insert(watch_id, paths);
            } else {
                if cfg!(feature = "debug-persistence") {
                    println!("Removing the watch with the id {}.", &watch_id);
                }

                unlisten_watch_by_id(self.fd, watch_id)
                    .with_context_of(|| format!("While informing the inotify instance to stop watching the folder {}:", path))
                    .print_err();
            }
        }
        self.watches = retained_watches;

        if cfg!(feature = "debug-persistence") {
            println!("The watches with the following IDs were retained: {}", format_list_of_active_ids(&self.watches));
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
            self.remove_watch(path);
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

/// Tells the file descriptor to stop listening to a watch with a certain id.
/// 
/// IMPORTANT: this function does NOT remove watch_id from `Inotify::watches`; that is the caller's job!
/// (WatchId being NonCopy gives some protection against accidentally calling it without having
/// removed it from self.watches, as well as the suggestiveness that this function does not accept
/// `self` as argument. Just don't try to dance around the protection.)
fn unlisten_watch_by_id(inotify_fd: RawFd, watch_id: WatchId) -> Result<(), SystemError> {
    // The error cases should be: self.fd is not valid, watch is not valid.
    // In either case, it is fine that watch is removed from self.watches in case of error.
    // self.watches.remove(&watch_id);
    let res = unsafe { libc::inotify_rm_watch(inotify_fd, watch_id.consume()) };

    if res < 0 {
        Err(std::io::Error::last_os_error().into())
    } else {
        Ok(())
    }
}

fn format_list_of_active_ids<T>(watches: &HashMap<WatchId, T>) -> String {
    let mut ids = watches.keys().map(|k| k.to_string()).collect::<Vec<_>>().join(", ");
    if ids.is_empty() {
        ids = "(none)".to_owned();
    }
    ids
}
