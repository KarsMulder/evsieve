// SPDX-License-Identifier: GPL-2.0-or-later

use std::io;
use std::fmt;

#[derive(Debug)]
pub struct ArgumentError {
    message: String,
}

impl ArgumentError {
    pub fn new(message: impl Into<String>) -> ArgumentError {
        ArgumentError { message: message.into() }
    }
}

impl fmt::Display for ArgumentError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Invalid argument: {}", first_letter_to_lowercase(self.message.clone()))
    }
}

#[derive(Debug)]
pub struct InternalError {
    message: String,
}

impl InternalError {
    pub fn new(message: impl Into<String>) -> InternalError {
        InternalError { message: message.into() }
    }
}

impl fmt::Display for InternalError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "An internal error occurred. This is most likely a bug.\nError message: {}", first_letter_to_lowercase(self.message.clone()))
    }
}

#[derive(Debug)]
pub enum RuntimeError {
    ArgumentError(ArgumentError),
    InternalError(InternalError),
    IoError(io::Error),
    /// The InterruptError signals that our program has been asked to stop using SIGINT or SIGTERM.
    InterruptError,
}

impl fmt::Display for RuntimeError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            RuntimeError::ArgumentError(error) => write!(f, "{}", error),
            RuntimeError::InternalError(error) => write!(f, "{}", error),
            RuntimeError::IoError(error)       => write!(f, "{}", error),
            RuntimeError::InterruptError       => write!(f, "Interrupt received."),
        }
    }
}


impl From<ArgumentError> for RuntimeError {
    fn from(error: ArgumentError) -> RuntimeError {
        RuntimeError::ArgumentError(error)
    }
}

impl From<InternalError> for RuntimeError {
    fn from(error: InternalError) -> RuntimeError {
        RuntimeError::InternalError(error)
    }
}

impl From<io::Error> for RuntimeError {
    fn from(error: io::Error) -> RuntimeError {
        RuntimeError::IoError(error)
    }
}

fn first_letter_to_lowercase(mut string: String) -> String {
    if let Some(first_char) = string.get_mut(0..1) {
        first_char.make_ascii_lowercase();
    }
    string
}