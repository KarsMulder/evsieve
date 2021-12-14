// SPDX-License-Identifier: GPL-2.0-or-later

#![allow(unused_imports)]
use std::os::raw::{c_int, c_char};
use std::process::{Command, Stdio};
use std::io::ErrorKind;
use std::sync::{Mutex, Barrier, Arc};

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

lazy_static! {
    /// This Mutex will be locked as long as an asynchronous attempt to notify systemd that we're ready
    /// is in progress, and is released when that attempt completes. As such, trying to lock this Mutex
    /// and then releasing that lock is a way to avoid interrupting notification in progress.
    static ref DAEMON_NOTIFICATION_IN_PROGRESS: Mutex<()> = Mutex::new(());
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
            // Use a barrier to wait until the thread notifies us that the notification lock has been
            // acquired before this function returns.
            let barrier = Arc::new(Barrier::new(2));
            let barrier_clone = Arc::clone(&barrier);

            std::thread::spawn(move || {
                // We don't care if the lock returns Err because it is poisoned.
                let _lock = DAEMON_NOTIFICATION_IN_PROGRESS.lock();
                barrier_clone.wait();

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

            barrier.wait();
        }
    }
}

/// Tries to notify the daemon that evsieve is ready. Depending on implementation, this notification may
/// happen asynchronously.
pub fn notify_ready() {
    notify("READY=1")
}

/// If notification is in progress, this function will wait until after it is completed.
pub fn await_completion() {
    drop(DAEMON_NOTIFICATION_IN_PROGRESS.lock());
}

pub fn is_available() -> bool {
    std::env::var("NOTIFY_SOCKET").is_ok()
}
