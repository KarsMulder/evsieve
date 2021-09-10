// SPDX-License-Identifier: GPL-2.0-or-later

mod systemd;

pub enum Daemon {
    None,
    Systemd,
}

impl Daemon {
    pub fn auto() -> Daemon {
        match systemd::is_available() {
            true => Daemon::Systemd,
            false => Daemon::None,
        }
    }

    pub fn finalize(&mut self) {
        match self {
            Daemon::None => {},
            Daemon::Systemd => systemd::notify_ready(),
        }
    }
}
