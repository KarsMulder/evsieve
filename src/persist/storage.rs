use std::ffi::CString;
use std::path::{PathBuf, Path};

use crate::error::{SystemError, Context};
use crate::predevice::PreInputDevice;

use super::blueprint::Blueprint;
use super::format::InvalidFormatError;

pub enum StorageError {
    /// Not enough environment variables were defined to be able to figure out where we should store the data.
    CouldNotFindStateDirectory,
}

pub fn load_blueprint(pre_device: PreInputDevice) -> Result<Blueprint, SystemError> {
    let capabilities_path = match capabilities_path_for_device(&pre_device.path.to_string_lossy()) {
        Ok(path) => path,
        Err(StorageError::CouldNotFindStateDirectory) => {
            // TODO (Medium Priority): Should this error message be specified somewhere else?
            return Err(SystemError::new("The environment variables do not give evsieve enough information to figure out where it is supposed to store its data. Please ensure that at least one of the following environment variables is defined: EVSIEVE_STATE_DIR, XDG_STATE_HOME, or HOME."))
        },
    }; // TODO (High Priority): Find something more elegant than to_string_lossy()
    
    let capabilities_data = match std::fs::read(&capabilities_path) {
        Ok(data) => data,
        Err(error) => match error.kind() {
            std::io::ErrorKind::NotFound => return Err(SystemError::new(format!(
                "No capabilities have been cached for the device \"{}\". In order to know which capabilities the created output devices must have, evsieve must know what the capabilities of the input devices are. These capabilities will be cached after the input device has been seen once. Please attach the specified device and then run evsieve again.",
                pre_device.path.display()
            ))),
            _ => return Err(SystemError::from(error).with_context(format!(
                "While trying to read the file \"{}\":",
                capabilities_path.display()
            ))),
        }
    };

    let capabilities = match crate::persist::format::decode(&capabilities_data) {
        Ok(data) => data,
        Err(InvalidFormatError) => return Err(SystemError::new(format!(
            "The data saved in the cached file \"{}\" has been corrupted. Please try restarting evsieve with the device \"{}\" to recreate the file. If this error keeps showing up, please file a bug report.",
            capabilities_path.display(), pre_device.path.display()
        ))),
    };

    Ok(Blueprint {
        pre_device,
        capabilities,
        // TODO (Critical): do something about the name.
        name: CString::new("(unknown)").unwrap()
    })
}

pub fn capabilities_path_for_device(device_path: &str) -> Result<PathBuf, StorageError> {
    let mut path = get_capabilities_path()?;
    path.push(format!("caps:{}", encode_path_for_device(device_path)));
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
