// SPDX-License-Identifier: GPL-2.0-or-later

use std::sync::Mutex;

pub fn split_once<'a>(value: &'a str, deliminator: &str) -> (&'a str, Option<&'a str>) {
    let mut splitter = value.splitn(2, deliminator);
    (splitter.next().unwrap(), splitter.next())
}

lazy_static!{
    static ref PRINTED_WARNINGS: Mutex<Vec<String>> = Mutex::new(Vec::new());
}

/// Prints a warning to the user using stderr, but only prints each unique message once.
pub fn warn_once(message: impl Into<String>) {
    let message: String = message.into();
    if let Ok(mut printed_warnings) = PRINTED_WARNINGS.lock() {
        if ! printed_warnings.contains(&message) {
            eprintln!("{}", message);
            printed_warnings.push(message);
        }
    } else {
        eprintln!("Warning: internal lock poisoned.");
        eprintln!("{}", message);
    }
}