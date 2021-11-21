// SPDX-License-Identifier: GPL-2.0-or-later

//! The persistence subsystem is in charge of taking Blueprints of unavailable InputDevices and waiting
//! until those devices become available so that the Blueprint can be opened. This happens in a separate
//! thread which communicates with the main thread through message passing.
//!
//! The system does not need to be launched until you actually want to open a blueprint, and ideally
//! should not be launched before then either, as that would waste system resources by having a useless
//! thread hanging around.

use crate::io::input::InputDevice;
use crate::io::internal_pipe;
use crate::io::internal_pipe::{Sender, Receiver};
use crate::persist::blueprint::Blueprint;
use crate::persist::inotify::Inotify;
use crate::persist::interface::HostInterface;
use crate::error::{Context, RuntimeError, SystemError};
use crate::io::epoll::PollMessage;
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

/// Reports that this subsystem sends back to the main thread.
pub enum Report {
    /// A device has been opened.
    DeviceOpened(InputDevice),
    /// A blueprint has been deemed unopenable and has been dropped.
    BlueprintDropped,
    /// This subsystem has shut down or almost shut down. There are no ongoing processes or destructors
    /// left to run that could cause trouble if the program were to exit() now.
    Shutdown,
}

enum Pollable<'a> {
    Command(&'a mut Receiver<Command>),
    Daemon(&'a mut Daemon),
}

impl<'a> AsRawFd for Pollable<'a> {
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

/// Launches the persistence subsystem and returns an interface to communicate with the main thread.
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


fn start_worker(mut comm_in: Receiver<Command>, comm_out: &mut Sender<Report>) -> Result<(), RuntimeError> {
    let mut daemon = Daemon::new()?;
    
    loop {
        let (commands, reports) = poll(&mut comm_in, &mut daemon)?;
        for command in commands {
            match command {
                Command::Shutdown => return Ok(()),
                Command::AddBlueprint(blueprint) => daemon.add_blueprint(blueprint)?,
            }
        }
        for report in reports {
            comm_out.send(report)?;
        }
    }
}

fn poll(comm_in: &mut Receiver<Command>, daemon: &mut Daemon)
        -> Result<(Vec<Command>, Vec<Report>), RuntimeError>
{
    let mut commands: Vec<Command> = Vec::new();
    let mut reports: Vec<Report> = Vec::new();

    let mut pollables = [Pollable::Daemon(daemon), Pollable::Command(comm_in)];
    let poll_result = crate::io::epoll::poll(&pollables)
        .with_context("While the persistence subsystem was polling for events:")?
        .collect::<Vec<PollMessage>>(); //TODO: needless collect?

    for (mut pollable, message) in pollables.iter_mut().zip(poll_result) {
        match message {
            PollMessage::Waiting => (),
            PollMessage::Broken => return Err(SystemError::new("Persistence daemon broken.").into()),
            PollMessage::Ready => match &mut pollable {
                Pollable::Daemon(daemon) => {
                    let TryOpenResult {
                        opened_devices,
                        broken_blueprints,
                        error_encountered,
                    } = daemon.try_open();

                    reports.extend(opened_devices.into_iter().map(Report::DeviceOpened));
                    reports.extend(broken_blueprints.into_iter().map(|_| Report::BlueprintDropped));

                    if let Some(error) = error_encountered {
                        // TODO: actually dispatch the reports even in case of error?
                        return Err(error);
                    }
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
    Ok((commands, reports))
}

struct TryOpenResult {
    opened_devices: Vec<InputDevice>,
    broken_blueprints: Vec<Blueprint>,
    error_encountered: Option<RuntimeError>,
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

    /// Checks whether it is possible to open some of the blueprints registered with this daemon,
    /// and opens them if it is.
    /// 
    /// Returns three things:
    /// 1. A Vec of all devices that were successfully opened and should be sent to the main thread.
    /// 2. A Vec of all blueprints that are considered "broken" and should not be tried to be opened again.
    /// 3. If something went wrong, an error. The Result<> wrapper is not used because we don't want
    ///    successfully opened devices to just disappear if an error happened later.
    fn try_open(&mut self) -> TryOpenResult {
        const MAX_TRIES: usize = 5;
        let mut result = TryOpenResult {
            opened_devices: Vec::new(),
            broken_blueprints: Vec::new(),
            error_encountered: None,
        };

        if let Err(error) = self.inotify.poll() {
            result.error_encountered = Some(error.into());
            return result;
        }

        for _ in 0 .. MAX_TRIES {
            // Try to open the devices.
            let mut remaining_blueprints = Vec::new();
            for blueprint in self.blueprints.drain(..) {
                match blueprint.try_open() {
                    Ok(Some(device)) => result.opened_devices.push(device),
                    Ok(None) => remaining_blueprints.push(blueprint),
                    Err(error) => {
                        error.print_err();
                        result.broken_blueprints.push(blueprint);
                    }
                }
            }
            self.blueprints = remaining_blueprints;
            
            // Just in case the relevant paths change between now and when we actually watch them
            // thanks to a race-condition, we do this within a loop until the paths are identical
            // for two iterations.
            match self.update_watches() {
                Ok(false) => return result, // The paths are identical.
                Ok(true) => (),             // The paths changed, we should re-scan.
                Err(error) => {             // Something went seriously wrong.
                    result.error_encountered = Some(error);
                    return result;
                }
            }
        }

        crate::utils::warn_once("Warning: maximum try count exceeded while listening for new devices.");
        result
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