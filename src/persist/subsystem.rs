// SPDX-License-Identifier: GPL-2.0-or-later

//! The persistence subsystem is in charge of taking Blueprints of unavailable InputDevices and waiting
//! until those devices become available so that the Blueprint can be opened. This happens in a separate
//! thread which communicates with the main thread through message passing.
//!
//! The system does not need to be launched until you actually want to open a blueprint, and ideally
//! should not be launched before then either, as that would waste system resources by having a useless
//! thread hanging around.

use crate::io::fd::HasFixedFd;
use crate::io::input::InputDevice;
use crate::io::internal_pipe;
use crate::io::internal_pipe::{Sender, Receiver};
use crate::persist::blueprint::{Blueprint, TryOpenBlueprintResult};
use crate::persist::inotify::Inotify;
use crate::persist::interface::HostInterface;
use crate::error::{Context, RuntimeError, SystemError};
use crate::io::epoll::{Epoll, FileIndex, Message};
use std::collections::HashSet;
use std::path::PathBuf;
use std::os::unix::io::{AsRawFd, RawFd};

/// Commands that the main thread can send to this subsystem.
#[allow(clippy::large_enum_variant)]
pub enum Command {
    /// Requests this subsystem to try to reopen this blueprint.
    AddBlueprint(Blueprint),
    /// Requests this subsystem to halt.
    Shutdown,
}

/// Reports that this subsystem sends back to the main thread.
#[allow(clippy::large_enum_variant)]
pub enum Report {
    /// A device has been opened.
    DeviceOpened(InputDevice),
    /// A blueprint has been deemed unopenable and has been dropped.
    BlueprintDropped,
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
unsafe impl HasFixedFd for Pollable {}

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


fn start_worker(comm_in: Receiver<Command>, comm_out: &mut Sender<Report>) -> Result<(), RuntimeError> {
    let daemon = Daemon::new()?;
    let mut epoll = Epoll::new()?;
    let daemon_index = epoll.add_file(Pollable::Daemon(daemon))?;
    epoll.add_file(Pollable::Command(comm_in))?;

    if cfg!(feature = "debug-persistence") {
        println!("Persistence subsystem launched.");
    }
    
    loop {
        let (commands, mut reports) = poll(&mut epoll, daemon_index)?;
        for command in commands {
            match command {
                Command::Shutdown => return Ok(()),
                Command::AddBlueprint(blueprint) => match &mut epoll[daemon_index] {
                    Pollable::Daemon(daemon) => {
                        daemon.add_blueprint(blueprint)?;
                        // Immediately try to open all blueprints after adding one, otherwise it is
                        // possible to fail to notice an blueprint becoming available if the associated
                        // events were already fired before it was added to the daemon.
                        try_open_and_report(daemon, &mut reports)?;
                    },
                    _ => unreachable!(),
                }
            }
        }
        for report in reports {
            comm_out.send(report)?;
        }
    }
}

fn poll(epoll: &mut Epoll<Pollable>, daemon_index: FileIndex) -> Result<(Vec<Command>, Vec<Report>), RuntimeError> {
    let mut commands: Vec<Command> = Vec::new();
    let mut reports: Vec<Report> = Vec::new();

    // If the feature debug-persistence has been enabled, then we will try to reopen all blueprints
    // periodically even if we were not notified they are ready.
    let timeout = if cfg!(feature = "debug-persistence") {
        5_000
    } else {
        crate::io::epoll::INDEFINITE_TIMEOUT
    };

    match epoll.poll(timeout) {
        Err(error) => {
            error.with_context("While the persistence subsystem was polling for events:").print_err();
            commands.push(Command::Shutdown);
        },
        Ok(messages) => {
            let messages: Vec<Message> = messages.collect();
            if ! messages.is_empty() {
                for message in messages {
                    match message {
                        Message::Broken(_index) => return Err(SystemError::new("Persistence daemon broken.").into()),
                        Message::Ready(index) | Message::Hup(index) => match &mut epoll[index] {
                            Pollable::Daemon(daemon) => {
                                daemon.poll()?;
                                try_open_and_report(daemon, &mut reports)?
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
            } else {
                // A timeout happened while polling.
                let daemon = match &mut epoll[daemon_index] {
                    Pollable::Command(_) => panic!("Internal invariant violated: daemon_index does not point to a Daemon"),
                    Pollable::Daemon(daemon) => daemon,
                };
                try_open_and_report(daemon, &mut reports)?
            }
        }
    }
    Ok((commands, reports))
}

/// A wrapper around Daemon::try_open() that conveniently turns opened/broken devices into reports and
/// has the standard Rust error handling mechanism. Notably, if an error is encountered at some point,
/// the `reports` vector may still be modified to include progress that was made before the error happened.
fn try_open_and_report(daemon: &mut Daemon, reports: &mut Vec<Report>) -> Result<(), RuntimeError> {
    let TryOpenResult {
        opened_devices,
        broken_blueprints,
        error_encountered,
    } = daemon.try_open();

    reports.extend(opened_devices.into_iter().map(Report::DeviceOpened));
    reports.extend(broken_blueprints.into_iter().map(|_| Report::BlueprintDropped));

    match error_encountered {
        Some(err) => Err(err),
        None => Ok(()),
    }
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

    /// Does nothing but clearing out the queued events. Call Daemon::try_open() to try to actually
    /// open the associated blueprints.
    pub fn poll(&mut self) -> Result<(), SystemError> {
        self.inotify.poll()
    }

    /// Checks whether it is possible to open some of the blueprints registered with this daemon,
    /// and opens them if it is.
    ///
    /// Does not clear out the associated Inotify's event queue. Make sure to call Daemon::poll() to do
    /// that as well in case an Epoll identifies this Daemon as ready.
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

        for _ in 0 .. MAX_TRIES {
            // Try to open the devices.
            let mut remaining_blueprints = Vec::new();
            for blueprint in self.blueprints.drain(..) {
                let blueprint_path = blueprint.pre_device.path.clone();
                let try_open_result = blueprint.try_open();

                if cfg!(feature = "debug-persistence") {
                    let result_as_str = match try_open_result {
                        TryOpenBlueprintResult::Success(_) => "success",
                        TryOpenBlueprintResult::NotOpened(_) => "not opened",
                        TryOpenBlueprintResult::Error(_, _) => "severe error",
                    };
                    println!("Attempted to open the device at {}. Outcome: {}", blueprint_path.to_string_lossy(), result_as_str);
                }

                match try_open_result {
                    TryOpenBlueprintResult::Success(device) => result.opened_devices.push(device),
                    TryOpenBlueprintResult::NotOpened(blueprint) => remaining_blueprints.push(blueprint),
                    TryOpenBlueprintResult::Error(blueprint, error) => {
                        error.print_err();
                        result.broken_blueprints.push(blueprint);
                    }
                }
            }
            self.blueprints = remaining_blueprints;
            
            let update_watch_result = self.update_watches();
            if cfg!(feature = "debug-persistence") {
                let result_as_str = match update_watch_result {
                    Ok(false) => "unchanged",
                    Ok(true) => "changed",
                    Err(_) => "severe error"
                };
                println!("Directory monitor status: {}", result_as_str);
            }

            // Just in case the relevant paths change between now and when we actually watch them
            // thanks to a race-condition, we do this within a loop until the paths are identical
            // for two iterations.
            match update_watch_result {
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

            if cfg!(feature = "debug-persistence") {
                let mut debug_str: String = paths_to_watch_hashset.iter().copied().cloned().collect::<Vec<_>>().join(", ");
                if debug_str.is_empty() {
                    debug_str = "(empty)".to_owned();
                }
                println!("Directories to monitor: {}", debug_str);
            }

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