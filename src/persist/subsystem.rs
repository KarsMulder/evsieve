// SPDX-License-Identifier: GPL-2.0-or-later

use crate::io::input::InputDevice;
use crate::persist::blueprint::Blueprint;
use crate::persist::inotify::Inotify;
use crate::error::{Context, RuntimeError, SystemError};
use std::collections::HashSet;
use std::path::PathBuf;
use std::os::unix::io::{AsRawFd, RawFd};

pub struct Daemon {
    blueprints: Vec<Blueprint>,
    inotify: Inotify,

    /// In case a device is successfully opened, store it here until it can be handled over to
    /// the main system.
    cached_devices: Vec<InputDevice>,
}

impl Daemon {
    pub fn new() -> Result<Daemon, SystemError> {
        Ok(Daemon {
            blueprints: Vec::new(),
            inotify: Inotify::new()?,
            cached_devices: Vec::new(),
        })
    }

    pub fn try_open(&mut self) -> Result<Vec<InputDevice>, RuntimeError> {
        const MAX_TRIES: usize = 5;
        let mut opened_devices: Vec<InputDevice> = Vec::new();

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
            
            // Find out which paths may cause a change, then watch them.
            // Just in case the relevant paths change between now and when we actually watch them
            // thanks to a race-condition, we do this within a loop until the paths are identical
            // for two iterations.
            let paths_to_watch: Vec<String> = self.get_paths_to_watch();
            let paths_to_watch_hashset: HashSet<&String> = paths_to_watch.iter().collect();
            let paths_already_watched: HashSet<&String> = self.inotify.watched_paths().collect();

            if paths_to_watch_hashset == paths_already_watched {
                return Ok(opened_devices);
            } else {
                self.inotify.set_watched_paths(paths_to_watch)?;
            }
        }

        crate::utils::warn_once("Warning: maximum try count exceeded while listening for new devices.");
        return Ok(opened_devices);
    }

    pub fn get_paths_to_watch(&mut self) -> Vec<String> {
        let mut traversed_directories: Vec<String> = Vec::new();

        for blueprint in self.blueprints.iter_mut() {
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