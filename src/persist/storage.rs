use std::os::unix::ffi::OsStrExt;
use std::path::{PathBuf, Path};

use crate::capability::Capabilities;
use crate::error::{SystemError, Context, RuntimeError, InternalError};
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
        let path_of_capabilities_file = match capabilities_path_for_device(&path_of_input_device) {
            Ok(path) => path,
            Err(StorageError::CouldNotFindStateDirectory) => {
                // TODO (Medium Priority): Should this error message be specified somewhere else?
                return Err(SystemError::new("The environment variables do not give evsieve enough information to figure out where it is supposed to store its data. Please ensure that at least one of the following environment variables is defined: EVSIEVE_STATE_DIR, XDG_STATE_HOME, or HOME."))
            },
        };
        
        let capabilities_data = read_capabilities(path_of_input_device, &path_of_capabilities_file)?;
        Ok(DeviceCache {
            location: path_of_capabilities_file,
            content: capabilities_data,
        })
    }


    /// Tells the cache which capabilities were just read from an input device that was opened. If those capabilities
    /// differ from what was cached, then the cache file shall be updated. Otherwise, nothing will happen.
    /// 
    /// All errors that happen in this function will be handled within this function.
    pub fn update_caps(&mut self, caps: &Capabilities, device_path: &Path) {
        // If the observed capabilities are equivalent to the cache, do nothing.
        match &self.content {
            CachedCapabilities::Known(known_caps) => {
                if known_caps.is_equivalent_to(caps) {
                    return;
                } else {
                    eprintln!(
                        "Notice: the capabilities that were cached on the disk for the device \"{}\" did not match the actual capabilities of this device. The cache shall now be updated.",
                        device_path.display()
                    );
                }
            },
            CachedCapabilities::NonExistent | CachedCapabilities::Corrupted => (),
        }

        let update_result = self.update_inner(caps).with_context_of(||
            format!("While trying to cache the capabilities of the device \"{}\":", device_path.display())
        );
        match update_result {
            Ok(()) => (),
            Err(error) => {
                error.print_err();
                eprintln!("Error: failed to cache the capabilities of an input device. Full persistence will not work.");
            }
        }
    }

    /// Internal function used by `update()`.
    fn update_inner(&mut self, caps: &Capabilities) -> Result<(), RuntimeError> {
        // The cache on disk does not match. Update it. First, serialize the capabilities to something that can be written.
        let caps_as_bytes = crate::persist::format::encode(&caps)
            .with_context("While trying to serialize the capabilities:")?;

        // Then make sure that the directory where we want to store the capabilities exists.
        let storage_dir = self.location.parent().ok_or_else(||
            InternalError::new("Cannot figure out the directory to which the device cache should be written. This is a bug.")
        )?;
        if ! storage_dir.exists() {
            match std::fs::create_dir_all(storage_dir) {
                Ok(()) => {
                    eprintln!("Info: creating the directory \"{}\" to store the cached capabilities of the input devices.", storage_dir.display());
                },
                Err(error) => {
                    return Err(SystemError::from(error).with_context(
                        format!("While trying to create the directory {}:", storage_dir.display())
                    ).into());
                }
            }    
        }

        // Finally, actually write the capabilities to a file.
        std::fs::write(&self.location, caps_as_bytes)
            .map_err(SystemError::from)
            .with_context_of(|| format!(
                "While trying to write to the file \"{}\":", &self.location.display()
            ))?;

        Ok(())
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

pub fn capabilities_path_for_device(device_path: &Path) -> Result<PathBuf, StorageError> {
    let mut path = get_capabilities_path()?;
    path.push(format!("{}", encode_path_for_device(device_path)));
    Ok(path)
}

/// Performs a map from a string to a string which has the following two properties:
/// 1. The output does not contain the character '/'.
/// 2. The mapping is deterministic and injective.
/// Tries to have the output resemble the input in a way that is sufficiently obvious for an observer.
fn encode_path_for_device(device_path: &Path) -> String {
    if let Some(path_str) = device_path.to_str() {
        path_str
            // It is assumed that the device path always starts with a '/' and this is currently enforced by evsieve.
            // This mapping is not injective in case that assumption is broken, but I'm going to return a path anyway
            // because even if I break that assumption and do not update this code, things will most likely work.
            // TODO (Medium Priority): Think of something better to do here.
            .trim_start_matches('/')
            .replace('\\', "\\\\")
            .replace('.', "\\.")
            .replace('/', ".")
    } else {
        // If the path is not valid UTF-8, I'm just going to dump the bytes of the path as-is, because this is a stupid
        // usecase that doesn't deserve any better level of support. Currently Rust's std doesn't even allow non-UTF-8
        // paths to be passed as argument.
        device_path.as_os_str().as_bytes().into_iter()
            .map(|byte| format!("\\b{:02X}", byte))
            .collect::<Vec<String>>().join("")
    }
}

/// Returns the path to the directory in which the capabilities of input devices must be cached.
fn get_capabilities_path() -> Result<PathBuf, StorageError> {
    let mut dir = get_state_path()?;
    if ! dir.has_root() {
        crate::utils::warn_once(format!("Warning: the state directory for evsieve has been defined as \"{}\", which is not an absolute path. This may have unexpected results.", dir.display()));
    }
    dir.push("device-cache");
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

#[test]
fn test_encode_path_for_device() {
    let bytes = [b'/', b'f', b'o', b'o'];
    let path = Path::new(std::ffi::OsStr::from_bytes(&bytes));
    assert_eq!(encode_path_for_device(path), "foo");

    let bytes = [1, 192, 20];
    let path = Path::new(std::ffi::OsStr::from_bytes(&bytes));
    assert_eq!(encode_path_for_device(path), "\\b01\\bC0\\b14");

    assert_eq!(encode_path_for_device(Path::new("/foo/bar/baz")), "foo.bar.baz");
    assert_eq!(encode_path_for_device(Path::new("/foo/bar.baz")), "foo.bar\\.baz");
    assert_eq!(encode_path_for_device(Path::new("/foo/bar\\.baz")), "foo.bar\\\\\\.baz");
}
