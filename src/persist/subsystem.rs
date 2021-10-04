// SPDX-License-Identifier: GPL-2.0-or-later

use crate::io::input::InputDevice;
use crate::io::internal_pipe;
use crate::io::internal_pipe::{Sender, Receiver};
use crate::persist::blueprint::Blueprint;
use crate::persist::inotify::Inotify;
use crate::error::{Context, InterruptError, RuntimeError, SystemError};
use crate::io::epoll::{Epoll, Message};
use std::collections::HashSet;
use std::path::PathBuf;
use std::os::unix::io::{AsRawFd, RawFd};
use std::thread::JoinHandle;

/// Commands that the main thread can send to this subsystem.
pub enum Command {
    /// Requests this subsystem to try to reopen this blueprint.
    AddBlueprint(Blueprint),
    /// Requests this subsystem to halt.
    Shutdown,
}

pub enum Report {
    /// A device has been opened.
    DeviceOpened(InputDevice),
    /// This subsystem has shut down or almost shut down. There are no ongoing processes or destructors
    /// left to run that could cause trouble if the program were to exit() now.
    Shutdown,
}

enum Pollable {
    Command(Receiver<Command>),
    Daemon(Daemon),
}
impl AsRawFd for Pollable {
    fn as_raw_fd(&self) -> RawFd {
        match self {
            Pollable::Command(receiver) => receiver.as_raw_fd(),
            Pollable::Daemon(daemon) => daemon.as_raw_fd(),
        }
    }
}

pub struct Daemon {
    blueprints: Vec<Blueprint>,
    inotify: Inotify,
}

pub fn launch() -> Result<HostInterface, SystemError> {
    let (commander, comm_in) = internal_pipe::channel()?;
    let (mut comm_out, reporter) = internal_pipe::channel()?;

    let join_handle = std::thread::spawn(move || {
        // TODO: report error
        start_worker(comm_in, &mut comm_out)
            .with_context("In the persistence subsystem:")
            .print_err();
        // TODO: consider panicking?
        comm_out.send(Report::Shutdown).print_err();
    });

    Ok(HostInterface { commander, reporter, join_handle })
}

/// The main thread controls the subsystem through this struct.
pub struct HostInterface {
    commander: Sender<Command>,
    reporter: Receiver<Report>,
    join_handle: JoinHandle<()>,
}

impl HostInterface {
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

    pub fn recv_opened_devices(&mut self) -> Result<Vec<InputDevice>, SystemError> {
        // TODO: think about error and shutdown handling.
        match self.reporter.recv()? {
            Report::DeviceOpened(device) => Ok(vec![device]),
            Report::Shutdown => Ok(Vec::new()),
        }
    }
}

impl AsRawFd for HostInterface {
    fn as_raw_fd(&self) -> RawFd {
        self.reporter.as_raw_fd()
    }
}

fn start_worker(comm_in: Receiver<Command>, comm_out: &mut Sender<Report>) -> Result<(), RuntimeError> {
    let daemon = Daemon::new()?;
    let mut epoll = Epoll::new()?;
    let daemon_index = unsafe {
        epoll.add_file(Pollable::Daemon(daemon))?
    };
    unsafe {
        epoll.add_file(Pollable::Command(comm_in))?
    };
    
    loop {
        let (commands, reports) = poll(&mut epoll)?;
        for command in commands {
            match command {
                Command::Shutdown => return Ok(()),
                Command::AddBlueprint(blueprint) => match &mut epoll[daemon_index] {
                    Pollable::Daemon(daemon) => daemon.add_blueprint(blueprint)?,
                    _ => unreachable!(),
                }
            }
        }
        for report in reports {
            comm_out.send(report)?;
        }
    }
}

fn poll(epoll: &mut Epoll<Pollable>) -> Result<(Vec<Command>, Vec<Report>), RuntimeError> {
    let mut commands: Vec<Command> = Vec::new();
    let mut reports: Vec<Report> = Vec::new();

    match epoll.poll() {
        Err(InterruptError {}) => {
            commands.push(Command::Shutdown);
        },
        Ok(messages) => for message in messages {
            match message {
                Message::Broken(_index) => return Err(SystemError::new("Persistence daemon broken.").into()),
                Message::Ready(index) => match &mut epoll[index] {
                    Pollable::Daemon(daemon) => {
                        let new_devices = daemon.try_open()?;
                        reports.extend(new_devices.into_iter().map(Report::DeviceOpened))
                    },
                    Pollable::Command(receiver) => {
                        match receiver.recv() {
                            Ok(command) => commands.push(command),
                            Err(error) => return Err(error.into()),
                        }
                    }
                }
            }
        }
    }
    Ok((commands, reports))
}

impl Daemon {
    pub fn new() -> Result<Daemon, SystemError> {
        Ok(Daemon {
            blueprints: Vec::new(),
            inotify: Inotify::new()?,
        })
    }

    pub fn add_blueprint(&mut self, blueprint: Blueprint) -> Result<(), RuntimeError> {
        self.blueprints.push(blueprint);
        self.update_watches()?;
        Ok(())
    }

    pub fn try_open(&mut self) -> Result<Vec<InputDevice>, RuntimeError> {
        const MAX_TRIES: usize = 5;
        let mut opened_devices: Vec<InputDevice> = Vec::new();
        self.inotify.poll()?;

        for _ in 0 .. MAX_TRIES {
            // Try to open the devices.
            self.blueprints.retain(|blueprint| {
                match blueprint.try_open() {
                    Ok(Some(device)) => {
                        opened_devices.push(device);
                        false
                    },
                    Ok(None) => true,
                    Err(error) => {
                        error.print_err();
                        false
                    }
                }
            });
            
            // Just in case the relevant paths change between now and when we actually watch them
            // thanks to a race-condition, we do this within a loop until the paths are identical
            // for two iterations.
            if ! self.update_watches()? {
                return Ok(opened_devices);
            }
        }

        crate::utils::warn_once("Warning: maximum try count exceeded while listening for new devices.");
        Ok(opened_devices)
    }

    /// Find out which paths may cause a change, then watch them.
    /// Returns true if the watched patch changed, otherwise returns false.
    fn update_watches(&mut self) ->  Result<bool, RuntimeError> {
        let paths_to_watch: Vec<String> = self.get_paths_to_watch();
            let paths_to_watch_hashset: HashSet<&String> = paths_to_watch.iter().collect();
            let paths_already_watched: HashSet<&String> = self.inotify.watched_paths().collect();

            if paths_to_watch_hashset == paths_already_watched {
                Ok(false)
            } else {
                self.inotify.set_watched_paths(paths_to_watch)?;
                Ok(true)
            }
    }

    pub fn get_paths_to_watch(&mut self) -> Vec<String> {
        let mut traversed_directories: Vec<String> = Vec::new();

        for blueprint in &mut self.blueprints {
            let paths = walk_symlink(blueprint.pre_device.path.clone());
            let mut directories = paths.into_iter()
                .filter_map(|mut path| {
                    path.pop();
                    // TODO: print a helpful error.
                    path.into_os_string().into_string().ok()
                });
            traversed_directories.extend(&mut directories);
        }

        traversed_directories.sort();
        traversed_directories.dedup();
        
        traversed_directories
    }
}

impl AsRawFd for Daemon {
    fn as_raw_fd(&self) -> RawFd {
        self.inotify.as_raw_fd()
    }
}

/// Returns a vector of all paths that lie in the chain of symlinks starting at `path`.
fn walk_symlink(path: PathBuf) -> Vec<PathBuf> {
    const MAX_SYMLINKS: usize = 20;

    // Walk down the chain of symlinks starting at path.
    let mut current_path: PathBuf = path.clone();
    let mut traversed_paths: Vec<PathBuf> = vec![current_path.clone()];

    while let Ok(next_path_rel) = current_path.read_link() {
        current_path.pop();
        current_path = current_path.join(next_path_rel);
        
        if traversed_paths.contains(&current_path) {
            break;
        }
        traversed_paths.push(current_path.clone());
        
        // The +1 is because the device node is not a symlink.
        if traversed_paths.len() > MAX_SYMLINKS + 1 {
            crate::utils::warn_once(format!(
                "Warning: too many symlinks encountered while resolving \"{}\".", path.display()
            ));
            break;
        }
    }

    traversed_paths
}