// SPDX-License-Identifier: GPL-2.0-or-later

use crate::event::Event;
use crate::io::input::InputDevice;
use crate::predevice::PreInputDevice;
use crate::capability::Capabilities;
use crate::error::SystemError;
use crate::io::epoll::{Pollable};
use std::collections::HashMap;
use std::os::unix::io::{AsRawFd, RawFd};
use std::path::PathBuf;

pub struct InputDeviceBlueprint {
    pub pre_device: PreInputDevice,
    pub capabilities: Capabilities,
}

impl InputDeviceBlueprint {
    /// Tries to reopen the device from which this blueprint was generated.
    /// On success, returns the device. On failure, returns Ok(None). In case of a grave
    /// error that signals reopening should not be retried, returns Err(SystemError).
    pub fn try_open(&mut self) -> Result<Option<InputDevice>, SystemError> {
        if ! self.pre_device.path.exists() {
            return Ok(None);
        }
        let input_device = match InputDevice::open(self.pre_device.clone()) {
            Ok(device) => device,
            Err(_) => return Ok(None),
        };
        if *input_device.capabilities() != self.capabilities {
            return Err(SystemError::new(
                format!("Cannot reopen input device \"{}\": this device's capabilities are different from the original device that disconnected.", self.pre_device.path.display())
            ));
        }
        Ok(Some(input_device))
    }
}

pub struct Inotify {
    fd: RawFd,
    watches: HashMap<String, i32>,
}

impl Inotify {
    pub fn new() -> Result<Inotify, SystemError> {
        let fd = unsafe { libc::inotify_init1(libc::IN_NONBLOCK) };
        if fd < 0 {
            return Err(SystemError::os_with_context("While initializing an inotify instance:"));
        }
        Ok(Inotify { fd, watches: HashMap::new() })
    }

    /// Returns an inotify that watches all interesting input directories.
    pub fn for_input_dirs() -> Result<Inotify, SystemError> {
        let mut inotify = Inotify::new()?;
        inotify.add_watch("/dev/input".into())?;
        inotify.add_watch("/dev/input/by-id".into())?;
        Ok(inotify)
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
        self.watches.insert(path, watch);
        Ok(())
    }

    pub fn remove_watch(&mut self, path: String) -> Result<(), SystemError> {
        let watch = match self.watches.remove(&path) {
            Some(value) => value,
            None => return Err(SystemError::new(format!("Cannot remove \"{}\" from inotify: this path is not watched.", path))),
        };

        // The error cases should be: self.fd is not valid, watch is not valid.
        // In either case, it is fine that watch has been removed from self.watches in case of error.
        let res = unsafe { libc::inotify_rm_watch(self.fd, watch) };
        if res < 0 {
            return Err(std::io::Error::last_os_error().into())
        }
        Ok(())
    }

    // Adds all watches in the given vector, and removes all not in the given vector.
    pub fn set_watches(&mut self, paths: Vec<String>) -> Result<(), SystemError> {
        let paths_to_add: Vec<String> = paths.iter()
            .filter(|&path| !self.watches.contains_key(path))
            .cloned().collect();
        let paths_to_remove: Vec<String> = self.watches.keys()
            .filter(|&path| !paths.contains(path))
            .cloned().collect();
        for path in paths_to_add {
            self.add_watch(path)?;
        }
        for path in paths_to_remove {
            self.remove_watch(path)?;
        }
        Ok(())
    }

    /// Does nothing besides clearing out the queued events.
    pub fn poll(&mut self) {
        // TODO: get this value from somewhere.
        const NAME_MAX: usize = 255;
        const BUFFER_SIZE: usize = std::mem::size_of::<libc::inotify_event>() + NAME_MAX + 1;
        let mut buffer: [u8; BUFFER_SIZE] = [0; BUFFER_SIZE];
        let res = unsafe {
            libc::read(self.fd, buffer.as_mut_ptr() as *mut libc::c_void, BUFFER_SIZE)
        };
        if res < 0 {
            eprintln!("Error: failed to read from an inotify instance.");
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

/// The BlueprintOpener is responsible for watching for filesystem events on all relevant directories
/// that are relevant for determining whether an input device can be reopened. 
pub struct BlueprintOpener {
    blueprint: InputDeviceBlueprint,
    inotify: Inotify,

    /// In case the device is successfully opened, store it here until it can be handled over to
    /// the epoll.
    cached_device: Option<Box<InputDevice>>,
}

impl BlueprintOpener {
    pub fn new(blueprint: InputDeviceBlueprint) -> Result<BlueprintOpener, SystemError> {
        let inotify = Inotify::new()?;
        let mut opener = BlueprintOpener { blueprint, inotify, cached_device: None };
        opener.update_watched_paths()?;
        Ok(opener)
    }

    pub fn try_open(&mut self) -> Result<Option<InputDevice>, SystemError> {
        if let Some(device) = self.blueprint.try_open()? {
            return Ok(Some(device));
        };
        // TODO: possible race condition: what if the filesystem changed before the watches were added?
        self.update_watched_paths()?;
        Ok(None)
    }

    pub fn update_watched_paths(&mut self) -> Result<(), SystemError> {
        const MAX_SYMLINKS: usize = 20;

        // Walk down the chain of symlinks starting at current_path.
        let mut current_path: PathBuf = self.blueprint.pre_device.path.clone();
        let mut traversed_paths: Vec<PathBuf> = vec![current_path.clone()];
        while let Ok(next_path_rel) = current_path.read_link() {
            current_path.pop();
            current_path = current_path.join(next_path_rel);
            traversed_paths.push(current_path.clone());
            // The +1 is because the device node is not a symlink.
            if traversed_paths.len() > MAX_SYMLINKS + 1 {
                return Err(SystemError::new("Too many symlinks."));
            }
        }
        
        // Watch every directory containing a symlink.
        let directories: Vec<String> = traversed_paths.into_iter()
            .map(|mut path| {
                path.pop();
                path.into_os_string().into_string().map_err(
                    |_| SystemError::new("Encountered a path without valid UTF-8 data.")
                )
            })
            .collect::<Result<Vec<String>, SystemError>>()?;
        self.inotify.set_watches(directories)?;
        
        Ok(())
    }
}

impl AsRawFd for BlueprintOpener {
    fn as_raw_fd(&self) -> RawFd {
        self.inotify.as_raw_fd()
    }
}

impl Pollable for BlueprintOpener {
    fn poll(&mut self) -> Result<Vec<Event>, Option<SystemError>> {
        self.inotify.poll();
        match self.try_open() {
            Ok(Some(device)) => {
                self.cached_device = Some(Box::new(device));
                Err(None)
            },
            Ok(None) => Ok(Vec::new()),
            Err(error) => Err(Some(error)),
        }
    }

    fn reduce(self: Box<Self>) -> Result<Box<dyn Pollable>, Option<SystemError>> {
        match self.cached_device {
            Some(device) => {
                eprintln!("The input device {} has been reopened.", device.path().display());
                Ok(device)
            },
            None => Err(Some(SystemError::new(format!(
                "Unable to reopen the input device {}.", self.blueprint.pre_device.path.display()
            )))),
        }
    }
}