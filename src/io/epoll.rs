// SPDX-License-Identifier: GPL-2.0-or-later

use crate::error::SystemError;
use crate::event::Event;
use crate::io::input::InputDevice;
use crate::io::persist::Inotify;
use crate::io::persist::BlueprintOpener;
use crate::sysexit;
use std::collections::HashMap;
use std::os::unix::io::{AsRawFd, RawFd};

/// The epoll is responsible for detecting which input devices have events available.
/// The evsieve program spends most of its time waiting on Epoll::poll, which waits until
/// some input device has events available.
/// 
/// It also keeps track of when input devices unexpectedly close. When some device closes,
/// it will be removed from the Epoll and returned as an EpollResult.
pub struct Epoll {
    fd: RawFd,
    files: HashMap<u64, Pollable>,
    /// A counter, so every file registered can get an unique index in the files map.
    counter: u64,
}

pub enum Pollable {
    InputDevice(InputDevice),
    Inotify(Inotify),
    BlueprintOpener(BlueprintOpener),
}

impl Pollable {
    pub fn poll(&mut self) -> Result<Vec<EpollResult>, SystemError> {
        Ok(match self {
            Pollable::InputDevice(device) => {
                device.poll()?.into_iter().map(EpollResult::Event).collect()
            },
            Pollable::Inotify(inotify) => {
                inotify.poll();
                vec![EpollResult::Inotify]
            },
            Pollable::BlueprintOpener(opener) => {
                if let Some(device) = opener.poll()? {
                    vec![EpollResult::DeviceOpened(device.into())]
                } else {
                    vec![]
                }
            },
        })
    }
}

impl AsRawFd for Pollable {
    fn as_raw_fd(&self) -> RawFd {
        match self {
            Pollable::InputDevice(device) => device.as_raw_fd(),
            Pollable::Inotify(device) => device.as_raw_fd(),
            Pollable::BlueprintOpener(device) => device.as_raw_fd(),
        }
    }
}

impl From<InputDevice> for Pollable {
    fn from(device: InputDevice) -> Pollable {
        Pollable::InputDevice(device)
    }
}

impl From<Inotify> for Pollable {
    fn from(inotify: Inotify) -> Pollable {
        Pollable::Inotify(inotify)
    }
}

pub enum EpollResult {
    /// An event has been received.
    Event(Event),
    /// A message has been received from a thread or interrupt.
    Interrupt,
    /// The Inotify told us that an interesting file or directory has changed.
    Inotify,
    /// Tells us that one of the input devices we're receiving events from has ceased working
    /// for some reason, most likely that reason being that the device has been physically
    /// disconnected from the computer.
    BrokenInputDevice(Box<InputDevice>),
    /// A BlueprintOpener has reopened an input device.
    DeviceOpened(Box<InputDevice>),
}

impl Epoll {
    pub fn new() -> Result<Epoll, SystemError> {
        let epoll_fd = unsafe {
            libc::epoll_create1(0)
        };
        if epoll_fd < 0 {
            return Err(SystemError::new("Failed to create epoll instance."));
        }

        Ok(Epoll {
            fd: epoll_fd,
            files: HashMap::new(),
            counter: 0,
        })
    }

    fn get_unique_index(&mut self) -> u64 {
        self.counter += 1;
        self.counter
    }

    /// # Safety
    /// The file must return a valid raw file descriptor.
    pub unsafe fn add_file(&mut self, file: Pollable) -> Result<(), SystemError> {
        let index = self.get_unique_index();
        let file_fd = file.as_raw_fd();

        // Sanity check: make sure we don't add a file that already belongs to this epoll.
        if self.files.values().any(|opened_file| opened_file.as_raw_fd() == file_fd) {
            return Err(SystemError::new("Cannot add a file to an epoll that already belongs to said epoll."));
        }
        self.files.insert(index, file);

        // We set the data to the index of said file, so we know which file is ready for reading.
        let mut event = libc::epoll_event {
            events: libc::EPOLLIN as u32,
            u64: index,
        };

        let result = libc::epoll_ctl(
            self.fd,
            libc::EPOLL_CTL_ADD,
            file_fd,
            &mut event,
        );

        if result < 0 {
            Err(SystemError::new("Failed to add a device to an epoll instance."))
        } else {
            Ok(())
        }
    }

    fn remove_file_by_index(&mut self, index: u64) -> Pollable {
        let file = match self.files.remove(&index) {
            Some(file) => file,
            None => panic!("Internal error: attempted to remove a device from an epoll that's not registered with it."),
        };

        let result = unsafe { libc::epoll_ctl(
            self.fd,
            libc::EPOLL_CTL_DEL,
            file.as_raw_fd(),
            std::ptr::null_mut(),
        )};

        if result < 0 {
            match std::io::Error::last_os_error().raw_os_error()
                    .expect("An unknown error occurred while removing a file from an epoll.") {
                // This file was not registered by this epoll.
                libc::ENOENT => eprintln!("Internal error: attempted to remove a device from an epoll that's not registered with it."),
                // There was not enough memory to carry out this operation.
                libc::ENOMEM => panic!("Out of kernel memory."),
                // The other error codes should never happen or indicate fundamentally broken invariants.
                _ => panic!("Failed to remove a file from an epoll: {}", std::io::Error::last_os_error()),
            }
        }

        file
    }

    /// Tries to read all events from all ready devices. Returns a vector containing all events read.
    /// If a device reports an error, said device is removed from self and also returned.
    pub fn poll(&mut self) -> Vec<EpollResult> {
        // The number 8 was chosen arbitrarily.
        let max_events: i32 = std::cmp::min(self.files.len(), 8) as i32;
        let mut events: Vec<libc::epoll_event> = (0 .. max_events).map(|_| libc::epoll_event {
            // The following values don't matter since the kernel will overwrite them anyway.
            // We're just initialzing them to make the compiler happy.
            events: 0, u64: 0
        }).collect();

        let result = unsafe {
            // Ensure that we cannot be interrupted by a signal in the short timespan between when
            // we check for the should_exit status, and when the epoll_pwait system call starts.
            let mut orig_sigmask: libc::sigset_t = std::mem::zeroed();
            let mut sigmask: libc::sigset_t = std::mem::zeroed();
            let orig_sigmask_mut_ptr = &mut orig_sigmask as *mut libc::sigset_t;
            let sigmask_mut_ptr = &mut sigmask as *mut libc::sigset_t;
            libc::sigemptyset(sigmask_mut_ptr);
            for &signal in sysexit::EXIT_SIGNALS {
                libc::sigaddset(sigmask_mut_ptr, signal);
            }
            libc::sigprocmask(libc::SIG_SETMASK, sigmask_mut_ptr, orig_sigmask_mut_ptr);

            if sysexit::should_exit() {
                return vec![EpollResult::Interrupt];
            }

            let result = libc::epoll_pwait(
                self.fd,
                events.as_mut_ptr(),
                max_events,
                -1, // timeout, -1 means it will wait indefinitely
                orig_sigmask_mut_ptr,
            );

            libc::sigprocmask(libc::SIG_SETMASK, orig_sigmask_mut_ptr, std::ptr::null_mut());
            result
        };

        if result < 0 {
            // Either we got an SIGINT/SIGTERM interrupt or an unexpected error.
            // It's unfortunately difficult to read errno from libc, so for now we assume the former.
            return vec![EpollResult::Interrupt];
        }

        let num_fds = result as usize;

        // Create a list of which devices are ready and which are broken.
        let mut ready_file_indices: Vec<u64> = Vec::new();
        let mut broken_file_indices: Vec<u64> = Vec::new();

        for event in events[0 .. num_fds].iter() {
            let file_index = event.u64;
            if event.events & libc::EPOLLIN as u32 != 0 {
                ready_file_indices.push(file_index);
            }
            if event.events & libc::EPOLLERR as u32 != 0 || event.events & libc::EPOLLHUP as u32 != 0 {
                broken_file_indices.push(file_index);
            }
        }

        // Retrieve all results from ready devices.
        let mut polled_results: Vec<EpollResult> = Vec::new();
        for index in ready_file_indices {
            if let Some(file) = self.files.get_mut(&index) {
                match file.poll() {
                    Ok(results) => polled_results.extend(results),
                    Err(error) => {
                        eprintln!("{}", error);
                        if ! broken_file_indices.contains(&index) {
                            broken_file_indices.push(index);
                        }
                    },
                }
            }
        }

        // Remove the broken devices from self and return them.
        polled_results.extend(
            broken_file_indices.into_iter()
            // Turn the broken indices into files.
            .map(
                |index| self.remove_file_by_index(index)
            )
            // Turn the broken files into results.
            .map(|pollable| {
                if let Pollable::InputDevice(device) = pollable {
                    let device_path = device.path();
                    eprintln!("The input device \"{}\" has been disconnected.", device_path.display());
                    EpollResult::BrokenInputDevice(Box::new(device))
                } else {
                    // TODO: can we recover from this?
                    panic!("Fatal error: an internal file descriptor broke.");
                }
            })
        );

        polled_results
    }

    pub fn get_input_devices_mut(&mut self) -> impl Iterator<Item=&mut Pollable> {
        self.files.iter_mut().map(
            |(_index, file)| file
        ).filter(|file| match file {
            Pollable::InputDevice(_device) => true,
            _ => false,
        })
    }

    /// Returns true if this epoll is polling at least one Inotify.
    pub fn has_inotify(&self) -> bool {
        self.files.values().any(
            |file| match file {
                Pollable::Inotify(_) => true,
                _ => false,
            }
        )
    }

    /// Removes all inotify instances from this epoll.
    pub fn clear_inotify(&mut self) {
        let mut indices_to_clear: Vec<u64> = Vec::new();
        for (index, file) in &self.files {
            if let Pollable::Inotify(_) = file {
                indices_to_clear.push(*index);
            }
        }
        for index in indices_to_clear {
            self.remove_file_by_index(index);
        }
    }

    /// Returns whether currently any files are opened under this epoll.
    pub fn has_files(&self) -> bool {
        ! self.files.is_empty()
    }
}

impl Drop for Epoll {
    fn drop(&mut self) {
        unsafe {
            libc::close(self.fd);
        }
    }
}