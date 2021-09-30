// SPDX-License-Identifier: GPL-2.0-or-later

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

lazy_static! {
    static ref SHOULD_EXIT_FLAG: Arc<AtomicBool> = Arc::new(AtomicBool::new(false));
}

pub fn should_exit() -> bool {
    SHOULD_EXIT_FLAG.load(Ordering::SeqCst)
}

/// The program should exit when it receives one of the following signals.
pub static EXIT_SIGNALS: &[i32] = &[
    libc::SIGINT,
    libc::SIGTERM,
    libc::SIGHUP,
];

/// Prepares to listen to exit signals.
pub fn init() -> Result<(), std::io::Error> {
    for &signal in EXIT_SIGNALS {
        signal_hook::flag::register(signal, Arc::clone(&SHOULD_EXIT_FLAG))?;
    }
    unsafe { libc::signal(libc::SIGPIPE, libc::SIG_IGN) };
    Ok(())
}