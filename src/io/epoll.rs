// SPDX-License-Identifier: GPL-2.0-or-later

use std::os::unix::io::{AsRawFd, RawFd};
use crate::error::{InternalError, RuntimeError, SystemError};
use crate::event::Event;
use crate::io::input::InputDevice;
use crate::io::persist::Inotify;
use crate::sysexit;
use std::collections::HashMap;

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
            }
        })
    }
}

impl AsRawFd for Pollable {
    fn as_raw_fd(&self) -> RawFd {
        match self {
            Pollable::InputDevice(device) => device.as_raw_fd(),
            Pollable::Inotify(device) => device.as_raw_fd(),
        }
    }
}

impl From<InputDevice> for Pollable {
    fn from(device: InputDevice) -> Pollable {
        Pollable::InputDevice(device)
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
    /// Must not add a file that already belongs to this Epoll, the file must
    /// return a valid raw file descriptor.
    pub unsafe fn add_file(&mut self, file: Pollable) -> Result<(), SystemError> {
        let index = self.get_unique_index();
        let file_fd = file.as_raw_fd();
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

    fn remove_file_by_index(&mut self, index: u64) -> Result<Pollable, RuntimeError> {
        let file = match self.files.remove(&index) {
            Some(file) => file,
            None => return Err(InternalError::new("Attempted to remove a device from an epoll that's not registered with it.").into()),
        };

        let result = unsafe { libc::epoll_ctl(
            self.fd,
            libc::EPOLL_CTL_DEL,
            file.as_raw_fd(),
            std::ptr::null_mut(),
        )};

        if result < 0 {
            Err(SystemError::new("Failed to remove a device from an epoll instance.").into())
        } else {
            Ok(file)
        }
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
            .filter_map(
                |index| match self.remove_file_by_index(index) {
                    Ok(file) => Some(file),
                    Err(error) => {
                        eprintln!("{}", error);
                        None
                    },
                }
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