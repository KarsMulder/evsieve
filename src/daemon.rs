use crate::error::Context;

// SPDX-License-Identifier: GPL-2.0-or-later

pub enum Daemon {
    None,
    Systemd,
}

impl Daemon {
    pub fn auto() -> Daemon {
        match std::env::var("NOTIFY_SOCKET") {
            Ok(_) => Daemon::Systemd,
            Err(_) => Daemon::None,
        }
    }

    /// # Safety
    /// This function can only be called from a single-threaded context.
    pub unsafe fn finalize(&mut self) {
        match self {
            Daemon::None => {},
            Daemon::Systemd => {
                match crate::subprocess::try_spawn("systemd-notify".to_string(), vec!["--ready".to_string()]) {
                    Ok(()) => {},
                    Err(error) => {
                        eprintln!("ERROR: failed to notify systemd that evsieve is ready.");
                        error.print_err();
                    },
                }
            },
        }
    }
}
