// SPDX-License-Identifier: GPL-2.0-or-later

use crate::io::input::InputDevice;
use crate::io::internal_pipe;
use crate::io::internal_pipe::{Sender, Receiver};
use crate::persist::blueprint::Blueprint;
use crate::persist::inotify::Inotify;
use crate::persist::interface::HostInterface;
use crate::error::{Context, RuntimeError, SystemError};
use crate::io::epoll::{Epoll, Message};
use std::collections::HashSet;
use std::path::PathBuf;
use std::os::unix::io::{AsRawFd, RawFd};

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
        // Asserting unwind safety for Sender. My reasons for this are a bit wobbly, but I looked at
        // its source and all visible actions it takes appear to be atomic, e.g. a message is either sent
        // or not. I can't think of a scenario where a panic at any point could violate safety.
        let panic_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            start_worker(comm_in, &mut comm_out)
                .with_context("In the persistence subsystem:")
                .print_err();
        }));

        comm_out.send(Report::Shutdown).print_err();

        if let Err(payload) = panic_result {
            std::panic::resume_unwind(payload);
        }
    });

    Ok(HostInterface { commander, reporter, join_handle })
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
        Err(error) => {
            error.with_context("While the persistence subsystem was polling for events:").print_err();
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
                    match path.into_os_string().into_string() {
                        Ok(string) => Some(string),
                        // Unfortunately the ill-designed Rust standard library does not provide means
                        // to convert a OsString to a CString without converting it to String first.
                        // This makes Evsieve unable to deal with non-UTF8 paths. This bug is sufficiently
                        // low-priority that I cannot be bothered to fix it until Rust fixes their standard
                        // library by adding direct OsString -> CString conversion.
                        Err(os_string) => {
                            let warning_message = format!(
                                "Error: unable to deal with non-UTF8 path \"{}\".",
                                os_string.to_string_lossy()
                            );
                            crate::utils::warn_once(warning_message);
                            None
                        },
                    }
                });
            traversed_directories.extend(&mut directories);
        }

        traversed_directories.sort_unstable();
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