// SPDX-License-Identifier: GPL-2.0-or-later

// Allowed because useless default implementations are dead lines of code.
#![allow(clippy::new_without_default)]

// Allowed because the key "" is a canonically valid key, and comparing a key to "" is more
// idiomatic than asking whether a key is empty.
#![allow(clippy::comparison_to_empty)]

// Allowed because nested ifs allow for more-readable code.
#![allow(clippy::collapsible_if)]

// Allowed because the matches! macro is not supported in Rust 1.41.1, under which evsieve must compile.
#![allow(clippy::match_like_matches_macro)]

// Disallowed for code uniformity.
#![warn(clippy::explicit_iter_loop)]

pub mod event;
pub mod key;
pub mod map;
pub mod domain;
pub mod state;
pub mod signal;
pub mod utils;
pub mod error;
pub mod capability;
pub mod range;
pub mod stream;
pub mod ecodes;
pub mod sysexit;
pub mod hook;
pub mod predevice;
pub mod print;
pub mod subprocess;
pub mod daemon;

pub mod io {
    pub mod input;
    pub mod epoll;
    pub mod output;
    pub mod loopback;
    pub mod persist;
    pub mod internal_pipe;
}

pub mod arguments {
    pub mod hook;
    pub mod parser;
    pub mod input;
    pub mod output;
    pub mod lib;
    pub mod map;
    pub mod toggle;
    pub mod print;
}

pub mod bindings {
    #[allow(warnings)]
    pub mod libevdev;
}

#[macro_use]
extern crate lazy_static;

use error::{InterruptError, RuntimeError, Context};
use io::epoll::{Epoll, Message};

fn main() {
    let result = run_and_interpret_exit_code();
    subprocess::terminate_all();
    std::process::exit(result)
}

fn run_and_interpret_exit_code() -> i32 {
    let result = std::panic::catch_unwind(run);

    match result {
        Ok(Ok(())) => 0,
        // A RuntimeError happened.
        Ok(Err(error)) => {
            eprintln!("{}", error);
            1
        },
        // A panic happened.
        Err(_) => {
            eprintln!("Internal error: a panic happened. This is a bug.");
            1
        },
    }
}

fn run() -> Result<(), RuntimeError> {
    sysexit::init()?;

    let args: Vec<String> = std::env::args().collect();
    if arguments::parser::check_help_and_version(&args) {
        daemon::notify_ready();
        return Ok(());
    }

    let (mut setup, input_devices) = arguments::parser::implement(args)?;
    let mut epoll = Epoll::new()?;
    for device in input_devices {
        unsafe { epoll.add_file(Box::new(device))? };
    }

    daemon::notify_ready();

    loop {
        let messages = match epoll.poll() {
            Ok(res) => res,
            Err(InterruptError {}) => return Ok(()),
        };

        for message in messages {
            match message {
                Message::Event(event) => stream::run(&mut setup, event),
                Message::BrokenDevice(device) => {
                    match device.reduce() {
                        Ok(file) => unsafe { epoll.add_file(file) }.print_err(),
                        Err(None) => (),
                        Err(Some(error)) => error.print_err(),
                    }
                }
            }
        }
    }
}