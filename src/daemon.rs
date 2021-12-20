// SPDX-License-Identifier: GPL-2.0-or-later

mod systemd;

pub fn notify_ready_async() {
    if systemd::is_available() {
        systemd::notify_ready();
    }
}
