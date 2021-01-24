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
        write!(f, "{}", self.message)
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
pub struct InterruptError {}

impl InterruptError {
    pub fn new() -> InterruptError {
        InterruptError {}
    }
}

impl fmt::Display for InterruptError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Interrupt received.")
    }
}

pub struct RuntimeError {
    pub context: Vec<String>,
    pub kind: RuntimeErrorKind,
}

impl RuntimeError {
    pub fn new(kind: RuntimeErrorKind) -> RuntimeError {
        RuntimeError {
            kind, context: Vec::new(),
        }
    }

    pub fn with_context(mut self, context: impl Into<String>) -> RuntimeError {
        self.context.insert(0, context.into());
        self
    }

    pub fn is_interrupt(&self) -> bool {
        match self.kind {
            RuntimeErrorKind::InterruptError(_) => true,
            _ => false,
        }
    }
}

pub trait WithContext<T> {
    fn with_context<S: Into<String>>(self, context: impl FnOnce() -> S) -> Result<T, RuntimeError>;
}

impl<T, E> WithContext<T> for Result<T, E> where E: Into<RuntimeError> {
    fn with_context<S: Into<String>>(self, context: impl FnOnce() -> S) -> Result<T, RuntimeError> {
        match self {
            Ok(value) => Ok(value),
            Err(error) => Err(error.into().with_context(context())),
        }
    }
}

#[derive(Debug)]
pub enum RuntimeErrorKind {
    ArgumentError(ArgumentError),
    InternalError(InternalError),
    IoError(io::Error),
    /// The InterruptError signals that our program has been asked to stop using SIGINT or SIGTERM.
    InterruptError(InterruptError),
}

impl fmt::Display for RuntimeError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match &self.kind {
            RuntimeErrorKind::ArgumentError(_)  => writeln!(f, "An error occured while parsing the arguments:")?,
            RuntimeErrorKind::InternalError(_)  => writeln!(f, "An internal error occured. This is most likely a bug. Error message:")?,
            RuntimeErrorKind::IoError(_)        => writeln!(f, "System error:")?,
            RuntimeErrorKind::InterruptError(_) => {
                return writeln!(f, "Interrupt received.")
            }
        };
        let mut indent: usize = 1;
        for context_line in &self.context {
            for _ in 0..indent {
                write!(f, "    ")?;
            }
            writeln!(f, "{}", context_line)?;
            indent += 1;
        }

        for _ in 0..indent {
            write!(f, "    ")?;
        }
        match &self.kind {
            RuntimeErrorKind::ArgumentError(error)  => write!(f, "{}", error),
            RuntimeErrorKind::InternalError(error)  => write!(f, "{}", error),
            RuntimeErrorKind::IoError(error)        => write!(f, "{}", error),
            RuntimeErrorKind::InterruptError(error) => write!(f, "{}", error),
        }
    }
}

impl From<ArgumentError> for RuntimeError {
    fn from(error: ArgumentError) -> RuntimeError {
        RuntimeError::new(RuntimeErrorKind::ArgumentError(error))
    }
}

impl From<InternalError> for RuntimeError {
    fn from(error: InternalError) -> RuntimeError {
        RuntimeError::new(RuntimeErrorKind::InternalError(error))
    }
}

impl From<io::Error> for RuntimeError {
    fn from(error: io::Error) -> RuntimeError {
        RuntimeError::new(RuntimeErrorKind::IoError(error))
    }
}

impl From<InterruptError> for RuntimeError {
    fn from(error: InterruptError) -> RuntimeError {
        RuntimeError::new(RuntimeErrorKind::InterruptError(error))
    }
}

fn first_letter_to_lowercase(mut string: String) -> String {
    if let Some(first_char) = string.get_mut(0..1) {
        first_char.make_ascii_lowercase();
    }
    string
}