// SPDX-License-Identifier: GPL-2.0-or-later

#![allow(unused_imports)]
use std::ffi::CString;
use std::os::raw::{c_int, c_char};
use std::process::{Command, Stdio};
use std::io::ErrorKind;
use std::sync::{Mutex, Barrier, Arc};

use crate::error::{SystemError, Context};

/// The systemd feature links statically against libsystemd instead of dynamically loading the function at runtime.
/// It is currently not enabled by default because it complicates the build process.
#[cfg(feature = "systemd")]
extern "C" {
    fn sd_notify(unset_environment: c_int, state: *const c_char) -> c_int;
}

#[cfg(feature = "systemd")]
fn notify(state: &str) -> Result<(), SystemError> {
    let state_cstring = std::ffi::CString::new(state).unwrap();
    let _result = unsafe { sd_notify(0, state_cstring.as_ptr()) }; // TODO
    Ok(())
}

/// Note: this function is expected to be called only once. Calling it multiple times has really bad performance
/// because it loads and unloads the libsystemd library every time. If you want to reuse this function in the
/// future, make sure to optimise it.
#[cfg(not(feature = "systemd"))]
fn notify(state: &str) -> Result<(), SystemError> {
    use crate::error::Context;

    // sd_notify() is part of sd-daemon.h and covered by the systemd stability promise.
    // See: https://systemd.io/PORTABILITY_AND_STABILITY/
    type SdNotifyFn = unsafe extern "C" fn(c_int, *const c_char) -> c_int;

    let libsystemd_name: CString = std::ffi::CString::new("libsystemd.so").unwrap();
    let sd_notify_name: CString = std::ffi::CString::new("sd_notify").unwrap();

    unsafe {
        let libsystemd = libc::dlopen(libsystemd_name.as_ptr(), libc::RTLD_LAZY);
        if libsystemd.is_null() { // TODO: use dlerr
            return dlerror().with_context("While trying to load the libsystemd library:");
        }

        // According to the man page, the correct way to check for an error during dlsym() is not to check
        // whether the return value is null, but instead to check whether dlerror() returns not-null.
        let _ = dlerror();
        let sd_notify_ptr = libc::dlsym(libsystemd, sd_notify_name.as_ptr());
        dlerror().with_context("While trying to load the sd_notify() symbol:")?;

        let state_cstring = std::ffi::CString::new(state).unwrap();

        {
            let sd_notify: SdNotifyFn = std::mem::transmute(sd_notify_ptr);
            sd_notify(0, state_cstring.as_ptr());
        }

        libc::dlclose(libsystemd);
    }

    Ok(())
}

lazy_static! {
    static ref DLERROR_MUTEX: Mutex<()> = Mutex::new(());
}

#[cfg(not(feature = "systemd"))]
fn dlerror() -> Result<(), SystemError> {
    use std::ffi::CStr;

    // Lock a mutex to make sure the memory allocated to dlerror() is not overwritten by a dlerror()
    // call from a different thread.
    let _guard = DLERROR_MUTEX.lock().expect("Internal mutex poisoned.");

    let res = unsafe { libc::dlerror() };
    if res.is_null() {
        return Ok(())
    }

    let description = unsafe { CStr::from_ptr(res) }.to_string_lossy();
    Err(SystemError::new(description))
}

/// Tries to notify the daemon that evsieve is ready. Depending on implementation, this notification may
/// happen asynchronously.
pub fn notify_ready() {
    match notify("READY=1") {
        Ok(()) => (),
        Err(error) => {
            eprintln!("Error: evsieve failed to notify the systemd daemon of being ready.");
            error.print_err();
        }
    }
}

pub fn is_available() -> bool {
    std::env::var("NOTIFY_SOCKET").is_ok()
}
