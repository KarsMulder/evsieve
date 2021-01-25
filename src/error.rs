// SPDX-License-Identifier: GPL-2.0-or-later

use std::io;
use std::fmt;

pub trait Context {
    fn context(&self) -> &[String];
    fn with_context(self, context: String) -> Self;
}

fn format_error_with_context(f: &mut fmt::Formatter, err_context: Vec<String>, err_msg: String) -> fmt::Result {
    let mut context_collapsed: Vec<String> = err_context;
    context_collapsed.push(err_msg);

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
                self.context.insert(0, context);
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

macro_rules! runtime_errors {
    ( $( $name:ident ),* ) => {
        $(
            context_error!($name);
        )*

        pub enum RuntimeError {
            $(
                $name ( $name ),
            )*
        }

        impl Context for RuntimeError {
            fn with_context(self, context: String) -> RuntimeError {
                match self {
                    $(
                        RuntimeError::$name(error)  => RuntimeError::$name(error.with_context(context)),
                    )*
                }
            }

            fn context(&self) -> &[String] {
                match self {
                    $(
                        RuntimeError::$name(error) => error.context(),
                    )*
                }
            }
        }

        $(
            impl From<$name> for RuntimeError {
                fn from(error: $name) -> RuntimeError {
                    RuntimeError::$name(error)
                }
            }
        )*
    };
}

runtime_errors!(ArgumentError, InternalError, SystemError);

impl From<io::Error> for SystemError {
    fn from(error: io::Error) -> SystemError {
        SystemError::new(format!("{}", error))
    }
}

impl From<io::Error> for RuntimeError {
    fn from(error: io::Error) -> RuntimeError {
        SystemError::from(error).into()
    }
}

pub struct InterruptError {}
impl InterruptError {
    pub fn new() -> InterruptError {
        InterruptError {}
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
        let err_message = match &self {
            RuntimeError::ArgumentError(error)  => format!("Invalid argument: {}", first_letter_to_lowercase(error.message.clone())),
            RuntimeError::InternalError(error)  => format!("Internal error: {}", first_letter_to_lowercase(error.message.clone())),
            RuntimeError::SystemError(error)    => format!("System error: {}", first_letter_to_lowercase(error.message.clone())),
        };
        format_error_with_context(f, self.context().to_owned(), err_message)
    }
}

fn first_letter_to_lowercase(mut string: String) -> String {
    if let Some(first_char) = string.get_mut(0..1) {
        first_char.make_ascii_lowercase();
    }
    string
}
