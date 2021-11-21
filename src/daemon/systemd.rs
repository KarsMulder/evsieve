// SPDX-License-Identifier: GPL-2.0-or-later

#![allow(unused_imports)]
use std::os::raw::{c_int, c_char};
use std::process::{Command, Stdio};
use std::io::ErrorKind;

/// The systemd feature links against libsystemd instead of falling back on the slower systemd-notify
/// tool. It is currently unused because it complicates the build process.
#[cfg(feature = "systemd")]
extern "C" {
    fn sd_notify(unset_environment: c_int, state: *const c_char) -> c_int;
}

#[cfg(feature = "systemd")]
fn notify(state: &str) {
    let state_cstring = std::ffi::CString::new(state).unwrap();
    let _result = unsafe { sd_notify(0, state_cstring.as_ptr()) };
}

#[cfg(not(feature = "systemd"))]
fn notify(state: &str) {
    // TODO: consider using the subprocess subsystem for this.
    if ! is_available() {
        return;
    }

    let child_res = Command::new("systemd-notify")
        .args(&["--", state])
        .stdin(Stdio::null())
        .spawn();

    match child_res {
        Err(error) => {
            match error.kind() {
                ErrorKind::NotFound => { eprintln!("Warning: the environment variable NOTIFY_SOCKET was set, suggesting that evsieve is being ran as a systemd service. However, the systemd-notify tool was not found. Evsieve is unable to notify the hypervisor that evsieve is ready."); },
                _ => { eprintln!("Warning: the environment variable NOTIFY_SOCKET was set, suggesting that evsieve is being ran as a systemd service. However, we failed to notify the systemd supervisor that evsieve is ready.\nError encountered: {}", error); },
            }
        },
        Ok(mut child) => {
            std::thread::spawn(move || {
                let notify_result = child.wait();
                match notify_result {
                    Ok(return_code) => {
                        if ! return_code.success() {
                            match return_code.code() {
                                Some(code) => eprintln!("Warning: the environment variable NOTIFY_SOCKET was set, suggesting that evsieve is being ran as a systemd service. However, the systemd-notify tool returned the following error code when we tried to inform the hypervisor that evsieve is ready: {}", code),
                                None => eprintln!("Warning: the environment variable NOTIFY_SOCKET was set, suggesting that evsieve is being ran as a systemd service. However, the systemd-notify tool failed to notify the hypervisor for an unknown reason."),
                            }
                        }
                    },
                    Err(error) => {
                        eprintln!("Warning: the environment variable NOTIFY_SOCKET was set, suggesting that evsieve is being ran as a systemd service. However, we failed to notify the systemd supervisor that evsieve is ready.\nError encountered: {}", error);
                    }
                }
            });
        }
    }
}

pub fn notify_ready() {
    notify("READY=1")
}

pub fn is_available() -> bool {
    std::env::var("NOTIFY_SOCKET").is_ok()
}
