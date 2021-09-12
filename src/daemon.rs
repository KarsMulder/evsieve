// SPDX-License-Identifier: GPL-2.0-or-later

mod systemd;

pub fn notify_ready() {
    if systemd::is_available() {
        systemd::notify_ready();
    }
}