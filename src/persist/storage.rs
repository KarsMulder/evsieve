use std::path::{PathBuf, Path};

use crate::capability::Capabilities;
use crate::error::{SystemError, Context};
use super::format::InvalidFormatError;

/// Represents information about an input device's capabilities that has been cached on the filesystem.
/// This has interfaces for reading the capabilities of input devices that are currently not available,
/// and for updating the current state of the cache in case it mismatches with the actual capabilities.
pub struct DeviceCache {
    /// The path to the file where we save the capabilities.
    pub location: PathBuf,
    /// What the file said the last time we read it or wrote to it.
    pub content: CachedCapabilities,
}

/// Represents the content of the cache file.
pub enum CachedCapabilities {
    /// The capabilities are cached and are equal to these.
    Known(Capabilities),
    /// No file caching the capabilities exists.
    NonExistent,
    /// A file caching the capabilities exists, but could its content could not be understood.
    Corrupted,
}

impl DeviceCache {
    pub fn load_for_input_device(path_of_input_device: &Path) -> Result<DeviceCache, SystemError> {
        let path_of_capabilities_file = match capabilities_path_for_device(&path_of_input_device.to_string_lossy()) {
            Ok(path) => path,
            Err(StorageError::CouldNotFindStateDirectory) => {
                // TODO (Medium Priority): Should this error message be specified somewhere else?
                return Err(SystemError::new("The environment variables do not give evsieve enough information to figure out where it is supposed to store its data. Please ensure that at least one of the following environment variables is defined: EVSIEVE_STATE_DIR, XDG_STATE_HOME, or HOME."))
            },
        }; // TODO (High Priority): Find something more elegant than to_string_lossy()
        
        let capabilities_data = read_capabilities(path_of_input_device, &path_of_capabilities_file)?;
        Ok(DeviceCache {
            location: path_of_capabilities_file,
            content: capabilities_data,
        })
    }
}

fn read_capabilities(path_of_input_device: &Path, path_of_capabilities_file: &Path) -> Result<CachedCapabilities, SystemError> {
    let capabilities_data = match std::fs::read(path_of_capabilities_file) {
        Ok(data) => data,
        Err(error) => match error.kind() {
            std::io::ErrorKind::NotFound => return Ok(CachedCapabilities::NonExistent),
            _ => return Err(SystemError::from(error).with_context(format!(
                "While trying to read the file \"{}\":",
                path_of_capabilities_file.display()
            ))),
        }
    };

    match crate::persist::format::decode(&capabilities_data) {
        Ok(data) => Ok(CachedCapabilities::Known(data)),
        Err(InvalidFormatError) => {
            eprintln!(
                "The capabilities for the device {} should have been saved in the cached file \"{}\", but the data in that file has been corrupted. We will try recreating that file at the first opportunity to do so. If this error keeps showing up, please file a bug report.",
                path_of_input_device.display(), path_of_capabilities_file.display(),
            );

            Ok(CachedCapabilities::Corrupted)
        },
    }
}

pub enum StorageError {
    /// Not enough environment variables were defined to be able to figure out where we should store the data.
    CouldNotFindStateDirectory,
}

pub fn capabilities_path_for_device(device_path: &str) -> Result<PathBuf, StorageError> {
    let mut path = get_capabilities_path()?;
    path.push(format!("caps:path={}", encode_path_for_device(device_path)));
    Ok(path)
}

/// Performs a map from a string to a string which has the following two properties:
/// 1. The output does not contain the character '/'.
/// 2. The mapping is deterministic and injective.
/// Tries to have the output resemble the input in a way that is sufficiently obvious for an observer.
fn encode_path_for_device(device_path: &str) -> String {
    device_path
        .replace('\\', "\\\\")
        .replace('.', "\\.")
        .replace('/', ".")
}

/// Returns the path to the directory in which the capabilities of input devices must be cached.
fn get_capabilities_path() -> Result<PathBuf, StorageError> {
    let mut dir = get_state_path()?;
    dir.push("capabilities");
    Ok(dir)
}

/// Looks for the evsieve state directory in the following order:
/// 1. $EVSIEVE_STATE_DIR
/// 2. /var/lib/evsieve (if root)
/// 3. $XDG_STATE_HOME/evsieve
/// 4. $HOME/.local/state/evsieve
/// 
/// For most purposes, you probably want to use `get_capabilities_path()` instead.
/// 
/// The reason that we're using the state dir instead of the cache dir is because the cache dir is allowed to be
/// wiped at any point in time, which can lead to problems like a device not being available upon reboot. Although
/// we call it "cache", this is a special type of cache that cannot just be regenerated whenever we need it.
/// Cache can be wiped because it is assumed that any application can just regenerate it, but since that assumption
/// is false in our case, it would be inappropriate to store these files in the cache.
fn get_state_path() -> Result<PathBuf, StorageError> {
    // First rule: if EVSIEVE_STATE_DIR is defined, use that directory no matter what.
    if let Some(dir) = std::env::var_os("EVSIEVE_STATE_DIR") {
        return Ok(dir.into());
    }

    // Second rule: if we're running as root, use /var/lib/evsieve/state.
    if is_running_as_root() {
        return Ok(Path::new("/var/lib/evsieve").to_owned());
    }

    // Third rule: otherwise, put it in the XDG state storage dir.
    if let Some(state_home) = std::env::var_os("XDG_STATE_HOME") {
        let mut dir: PathBuf = state_home.into();
        dir.push("evsieve");
        return Ok(dir);
    }

    // If XDG_STATE_HOME is not defined, fall back to the XDG defined default of $HOME/.local/state
    let mut dir: PathBuf = match std::env::var_os("HOME") {
        Some(dir) => dir.into(),
        None => return Err(StorageError::CouldNotFindStateDirectory),
    };
    dir.push(".local/state");
    Ok(dir)
}

/// Checks if this program is running as root.
fn is_running_as_root() -> bool {
    let euid = unsafe { libc::geteuid() };
    euid == 0
}
