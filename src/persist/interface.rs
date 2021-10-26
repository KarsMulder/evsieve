use crate::persist::subsystem::{Command, Report};
use crate::persist::blueprint::Blueprint;
use crate::io::internal_pipe::{Sender, Receiver};
use crate::io::epoll::{Epoll, FileIndex};
use crate::{Pollable, error::*};
use std::thread::JoinHandle;
use std::os::unix::io::{AsRawFd, RawFd};

/// The main thread controls the persistence subsystem through this struct.
pub struct HostInterface {
    pub(super) commander: Sender<Command>,
    pub(super) reporter: Receiver<Report>,
    pub(super) join_handle: JoinHandle<()>,
}

impl HostInterface {
    /// Asks the subsystem to try to reopen this blueprint.
    pub fn add_blueprint(&mut self, blueprint: Blueprint) -> Result<(), SystemError> {
        self.commander.send(Command::AddBlueprint(blueprint))
    }

    /// Asks the subsystem to start shutting down. Does not wait until it has actually shut down.
    pub fn request_shutdown(&mut self) -> Result<(), SystemError> {
        self.commander.send(Command::Shutdown)
    }

    pub fn await_shutdown(mut self) {
        if self.request_shutdown().is_ok() {
            let _ = self.join_handle.join();
        }
    }

    pub fn recv(&mut self) -> Result<Report, SystemError> {
        self.reporter.recv()
    }
}

impl AsRawFd for HostInterface {
    fn as_raw_fd(&self) -> RawFd {
        self.reporter.as_raw_fd()
    }
}

pub enum HostInterfaceState {
    /// The persistence subsystem has never been started yet because it wasn't needed so far.
    NotStarted,
    /// The persistence subsystem is currently running and registered with a certain Epoll at a given index.
    Running(FileIndex),
    /// The persistence subsystem has crashed.
    Error,
    /// The persistence subsystem has successfully shut down.
    Shutdown,
}

impl HostInterfaceState {
    pub fn new() -> HostInterfaceState {
        HostInterfaceState::NotStarted
    }

    /// Returns a reference to a HostInterface registered with a certain Epoll. Never call this
    /// function with two different epolls through the lifetime of self.
    pub fn require<'a>(&mut self, epoll: &'a mut Epoll<Pollable>) -> Option<&'a mut HostInterface> {
        use HostInterfaceState::*;

        // Start the subsystem if it is not already running.
        if let NotStarted = self {
            let interface = match crate::persist::subsystem::launch() {
                Ok(interface) => interface,
                Err(error) => {
                    eprintln!("Warning: failed to start the persistence subsystem. Devices with the persist flag may not be (re)opened successfully.");
                    error.print_err();
                    *self = Error;
                    return None;
                }
            };
            let index = match unsafe { epoll.add_file(crate::Pollable::PersistSubsystem(interface)) } {
                Ok(index) => index,
                Err(error) => {
                    error.with_context("While adding the persistence subsystem interface to an epoll:").print_err();
                    *self = Error;
                    return None;
                }
            };
            *self = Running(index);
        }

        self.get(epoll)
    }

    pub fn get<'a>(&mut self, epoll: &'a mut Epoll<Pollable>) -> Option<&'a mut HostInterface> {
        use HostInterfaceState::*;
        match self {
            Running(index) => {
                if let Some(crate::Pollable::PersistSubsystem(ref mut interface)) = epoll.get_mut(*index) {
                    Some(interface)
                } else {
                    None
                }
            },
            NotStarted => None,
            Error => None,
            Shutdown => None,
        }
    }

    pub fn mark_as_broken(&mut self) {
        *self = HostInterfaceState::Error;
    }

    pub fn mark_as_shutdown(&mut self) {
        *self = HostInterfaceState::Shutdown;
    }

    pub fn await_shutdown(self, epoll: &mut Epoll<Pollable>) {
        if let HostInterfaceState::Running(index) = self {
            if let Some(Pollable::PersistSubsystem(interface)) = epoll.remove(index) {
                interface.await_shutdown();
            }
        }
    }
}