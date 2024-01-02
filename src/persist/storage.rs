use std::path::{PathBuf, Path};

pub enum StorageError {
    /// Not enough environment variables were defined to be able to figure out where we should store the data.
    CouldNotFindStateDirectory,
}

pub fn capabilities_path_for_device(device_path: &str) -> Result<PathBuf, StorageError> {
    let mut path = get_capabilities_path()?;
    path.push(encode_path_for_device(device_path));
    Ok(path)
}

fn encode_path_for_device(device_path: &str) -> String {
    // Tries to assign a unique name to each device path that contains no / characters. We do this by either of the
    // following two methods:
    // Method 1: substitute '/' -> '__'
    // Method 2: substitute '/' -> '__', '_' -> '\_', '\' -> '\\'
    //
    // If the device path does not contain any of the following strings, then the first method is safe: no two path
    // different will be mapped to the same name under the first method:
    //    '__', '_/', '/_', '\'
    // (Proof: the first method is reversible if nothing other than '/' could generate '__'. The first method is not
    // used if the original contains a '__' sequence, nor is it used if any '_' character is adjacent to anything
    // would be mapped into another '_' character.)
    //
    // If the path does contain any of the above strings, the second method must be used. The second method maps
    // everything to an unique name, and they never conflict with the names we generated using the first method.
    // (Proof: we only use the second method if the path contains '_' or '\', and under the second method, those
    // all get mapped to something with '\', however the first method is never used on files that contain a '\'.)
    let dangerous_patterns = ["__", "_/", "/_", "\\"];
    if dangerous_patterns.iter().any(|pat| device_path.contains(pat)) {
        // Use method #2
        device_path
            .replace('\\', "\\\\")
            .replace('_', "\\_")
            .replace('/', "__")
    } else {
        // Use method #1
        device_path.replace('/', "__")
    }
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
