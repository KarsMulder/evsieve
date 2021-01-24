// SPDX-License-Identifier: GPL-2.0-or-later

#![allow(clippy::new_without_default)]
#![allow(clippy::comparison_to_empty)]
#![allow(clippy::collapsible_if)]
#![warn(clippy::explicit_iter_loop)]

pub mod event;
pub mod key;
pub mod map;
pub mod domain;
pub mod state;
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

pub mod io {
    pub mod input;
    pub mod epoll;
    pub mod output;
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

use error::{RuntimeError, RuntimeErrorKind};

fn main() {
    let result = run_and_interpret_exit_code();
    std::process::exit(result)
}

fn run_and_interpret_exit_code() -> i32 {
    let result = match run() {
        Ok(_) => 0,
        Err(error) => match error.kind {
            RuntimeErrorKind::InterruptError(_) => 0,
            RuntimeErrorKind::IoError(io_error) => {
                eprintln!("{}", io_error);
                1
            },
            RuntimeErrorKind::ArgumentError(arg_error) => {
                eprintln!("{}", arg_error);
                1
            },
            RuntimeErrorKind::InternalError(internal_error) => {
                eprintln!("{}", internal_error);
                1
            }
        },
    };
    subprocess::terminate_all();
    result
}

fn run() -> Result<(), RuntimeError> {
    sysexit::init()?;
    let mut setup = arguments::parser::implement(std::env::args().collect())?;
    loop {
        stream::run(&mut setup)?;
    }
}