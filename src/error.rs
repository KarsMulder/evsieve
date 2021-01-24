// SPDX-License-Identifier: GPL-2.0-or-later

use std::io;
use std::fmt;

pub trait Context {
    fn context(&self) -> &[String];
    fn with_context(self, context: String) -> Self;
}

fn format_error_with_context(f: &mut fmt::Formatter, err_header: impl Into<String>, err_context: &[String], err_msg: impl Into<String>) -> fmt::Result {
    let mut context_collapsed: Vec<String> = Vec::new();
    context_collapsed.push(err_header.into());
    context_collapsed.extend(err_context.iter().cloned());
    context_collapsed.push(err_msg.into());

    for (indent, context_line) in context_collapsed.into_iter().enumerate() {
        for _ in 0..indent {
            write!(f, "    ")?;
        }
        writeln!(f, "{}", context_line)?;
    }

    Ok(())
}

macro_rules! context_error {
    ($name:ident) => {
        #[derive(Debug)]
        pub struct $name {
            context: Vec<String>,
            message: String,
        }
        impl $name {
            pub fn new(message: impl Into<String>) -> Self {
                Self { message: message.into(), context: Vec::new() }
            }
        }
        impl Context for $name {
            fn context(&self) -> &[String] {
                &self.context
            }

            fn with_context(mut self, context: String) -> Self {
                self.context.push(context);
                self
            }
        }
        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
                write!(f, "{}", self.message)
            }
        }
    };
}

context_error!(ArgumentError);
context_error!(InternalError);
context_error!(SystemError);

impl From<io::Error> for SystemError {
    fn from(error: io::Error) -> Self {
        SystemError::new(format!("{}", error))
    }
}

pub struct InterruptError {}
impl InterruptError {
    pub fn new() -> InterruptError {
        InterruptError {}
    }
}

pub enum RuntimeError {
    ArgumentError(ArgumentError),
    InternalError(InternalError),
    SystemError(SystemError),
}

impl Context for RuntimeError {
    fn with_context(self, context: String) -> RuntimeError {
        match self {
            RuntimeError::ArgumentError(error)  => RuntimeError::ArgumentError(error.with_context(context)),
            RuntimeError::InternalError(error)  => RuntimeError::InternalError(error.with_context(context)),
            RuntimeError::SystemError(error)    => RuntimeError::SystemError(error.with_context(context)),
        }
    }

    fn context(&self) -> &[String] {
        match self {
            RuntimeError::ArgumentError(error)  => error.context(),
            RuntimeError::InternalError(error)  => error.context(),
            RuntimeError::SystemError(error)    => error.context(),
        }
    }
}

impl<T, E> Context for Result<T, E> where E: Context {
    fn with_context(self, context: String) -> Self {
        match self {
            Ok(value) => Ok(value),
            Err(error) => Err(error.with_context(context)),
        }
    }

    fn context(&self) -> &[String] {
        match self {
            Ok(_) => &[],
            Err(error) => error.context(),
        }
    }
}



impl fmt::Display for RuntimeError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let err_header = match self {
            RuntimeError::ArgumentError(_)  => "An error occured while parsing the arguments:",
            RuntimeError::InternalError(_)  => "An internal error occured. This is most likely a bug. Error message:",
            RuntimeError::SystemError(_)    => "System error:",
        };
        let err_message = match &self {
            RuntimeError::ArgumentError(error)  => format!("{}", error),
            RuntimeError::InternalError(error)  => format!("{}", error),
            RuntimeError::SystemError(error)    => format!("{}", error),
        };
        format_error_with_context(f, err_header, self.context(), err_message)
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
        RuntimeError::SystemError(error.into())
    }
}

impl From<SystemError> for RuntimeError {
    fn from(error: SystemError) -> RuntimeError {
        RuntimeError::SystemError(error)
    }
}
