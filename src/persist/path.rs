use std::{path::{PathBuf, Path}, ffi::OsString};
use std::os::unix::prelude::*;
use crate::error::*;

use crate::error::SystemError;

/// When we want to store the persistence information of the device in device_path,
/// the file name of the file we use for it is computed in this function.
/// 
/// This function is injective: it ensures that each device path gets its own storage
/// name. However, it is possible that the resulting name cannot be used because of
/// path length restrictions.
pub fn device_path_to_storage_name(device_path: &Path) -> PathBuf {
    let mut result_bytes: Vec<u8> = Vec::new();
    // Escape / to _, escape _ to \_, and escape \ to \\.
    for &byte in device_path.as_os_str().as_bytes() {
        match byte {
            b'/' => result_bytes.push(b'_'),
            b'_' => result_bytes.extend([b'\\', b'_']),
            b'\\' => result_bytes.extend([b'\\', b'\\']),
            _ => result_bytes.push(byte),
        }
    }

    OsString::from_vec(result_bytes).into()
}

/// Checks if evsieve is being ran as root.
fn is_root() -> bool {
    unsafe { libc::geteuid() == 0 }
}

/// # Panics
/// May panic if the operating system is not POSIX-compliant.
fn get_xdg_data_dir() -> PathBuf {
    if let Ok(path) = std::env::var("XDG_DATA_HOME") {
        path.into()
    } else if let Ok(path) = std::env::var("HOME") {
        Path::new(&path).join(".local/share")
    } else {
        panic!("The $HOME environment variable is not set. This is in violation of the POSIX specification.");
    }
}

/// Returns the path to the directory where we shall store all persistent data.
/// NOTE: This is not the directory for storing prototypes of input devices.
/// Use `get_device_cache_dir()` for that instead.
fn get_storage_dir() -> PathBuf {
    if is_root() {
        "/var/lib/evsieve".into()
    } else {
        // Even though we call it "cache", we do not store it in the .cache dir because
        // the user could find serious hindrance if the cache were arbitrarily cleaned.
        get_xdg_data_dir().join("evsieve")
    }
}

/// Returns the path to the directory where we shall store all persistent data.
/// If the directory does not exist, create it.
fn require_storage_dir() -> Result<PathBuf, SystemError> {
    let path = get_storage_dir();
    if ! path.exists() {
        create_dir_all_systemerror(&path)?;
        eprintln!("Info: the directory {} has been created for persistent data storage.", path.to_string_lossy());
        Ok(path)
    } else {
        Ok(path)
    }
}

/// Like std::fs::create_dir_all(), but returns a SystemError with context.
fn create_dir_all_systemerror(path: &Path) -> Result<(), SystemError> {
    std::fs::create_dir_all(path)
        .map_err(SystemError::from)
        .with_context_of(|| format!("While trying to create the directory {}:", path.to_string_lossy()))
}

const DEVICE_CACHE_NAME: &str = "device-cache";

/// Returns the name of the folder where we intend to store prototypes of devices that have
/// been tagged with the persist flag.
pub fn get_device_cache_dir() -> PathBuf {
    get_storage_dir().join(DEVICE_CACHE_NAME)
}

/// Returns the name of the folder where we intend to store prototypes of devices that have
/// been tagged with the persist flag. Creates the directory if it didn't exist already.
pub fn require_device_cache_dir() -> Result<PathBuf, SystemError> {
    let path = require_storage_dir()
        .with_context("While trying to find evsieve's storage directory:")?
        .join(DEVICE_CACHE_NAME);

    if !path.exists() {
        create_dir_all_systemerror(&path)?;
    }

    Ok(path)
}
