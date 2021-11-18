// SPDX-License-Identifier: GPL-2.0-or-later

//! In Evsieve 1.0.0, the program would automatically exit if there were no input devices to poll
//! events from. For the sake of backwards compatibility, this behavious must be maintained. Since
//! version 1.3.0, this becomes more difficult due to the added persistence mechanism, making it no
//! longer trivial to determine whether evsieve can still receive events without running afoul of
//! race conditions.
//! 
//! As such, this module is introduced: each ActivityLink represents a reason why Evsieve should
//! not automatically exit now. E.g. each input device should contain an ActivityLink to signify
//! Evsieve should not exit because it can still receive events from this device.
//! 
//! ActivityLinks do not block Evsieve from exiting; it will still exit if asked to (e.g. because
//! of Ctrl+C), in merely block an automatic exit.

use std::sync::atomic::{AtomicUsize, Ordering};

static NUM_ACTIVE_LINKS: AtomicUsize = AtomicUsize::new(0);

pub struct ActivityLink {
    /// Make sure ActivityLink cannot be constructed outside this module.
    _private: (),
}

impl ActivityLink {
    pub fn new() -> ActivityLink {
        // I am not sure what the right ordering is, so I just play it safe and use Ordering::SeqCst.
        // I doubt this code is ran often enough to have any remotely measurable performance impact.
        NUM_ACTIVE_LINKS.fetch_add(1, Ordering::SeqCst);
        ActivityLink { _private: () }
    }
}

impl Drop for ActivityLink {
    fn drop(&mut self) {
        NUM_ACTIVE_LINKS.fetch_sub(1, Ordering::SeqCst);
    }
}

pub fn num_active_links() -> usize {
    NUM_ACTIVE_LINKS.load(Ordering::SeqCst)
}