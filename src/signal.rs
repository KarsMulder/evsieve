// SPDX-License-Identifier: GPL-2.0-or-later

use std::sync::{Arc, Weak, Mutex};

/// As long as a SignalBlock exists, this program will not receive any signals unless it asks
/// for them. Only one SignalBlock should ever exist simultaneously, having more of them is
/// a logical error that can permanently destroy the program's ability to receive signals.
///
/// Use signal::block() to get a SingleTon of this struct.
pub struct SignalBlock {
    orig_sigmask: libc::sigset_t,
}

lazy_static! {
    static ref SIGNAL_BLOCK: Mutex<Weak<SignalBlock>> = Mutex::new(Weak::new());
}

pub fn block() -> Arc<SignalBlock> {
    let mut lock = SIGNAL_BLOCK.lock().expect("Internal mutex poisoned.");
    match lock.upgrade() {
        Some(block) => block,
        None => {
            let block = Arc::new(SignalBlock::new());
            *lock = Arc::downgrade(&block);
            block
        },
    }
}

impl SignalBlock {
    fn new() -> SignalBlock {
        unsafe {
            let mut orig_sigmask: libc::sigset_t = std::mem::zeroed();
            let mut sigmask: libc::sigset_t = std::mem::zeroed();
            let orig_sigmask_mut_ptr = &mut orig_sigmask as *mut libc::sigset_t;
            let sigmask_mut_ptr = &mut sigmask as *mut libc::sigset_t;
        
            libc::sigfillset(sigmask_mut_ptr);
            libc::sigdelset(sigmask_mut_ptr, libc::SIGSEGV);
            libc::sigprocmask(libc::SIG_SETMASK, sigmask_mut_ptr, orig_sigmask_mut_ptr);

            SignalBlock { orig_sigmask }
        }
    }

    pub fn orig_sigmask(&self) -> &libc::sigset_t {
        &self.orig_sigmask
    }
}

impl Drop for SignalBlock {
    fn drop(&mut self) {
        unsafe {
            let orig_sigmask_ptr = &self.orig_sigmask as *const libc::sigset_t;
            libc::sigprocmask(libc::SIG_SETMASK, orig_sigmask_ptr, std::ptr::null_mut());
        }
    }
}
