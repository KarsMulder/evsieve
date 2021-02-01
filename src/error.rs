// SPDX-License-Identifier: GPL-2.0-or-later

use std::io;
use std::fmt;
use std::fmt::Write;

pub trait Context {
    fn context(&self) -> &[String];
    fn with_context<T: Into<String>>(self, context: T) -> Self;
    fn print_err(self);
}

fn format_error_with_context(f: &mut fmt::Formatter, err_context: Vec<String>, err_msg: String) -> fmt::Result {
    let mut context_collapsed: Vec<String> = err_context;
    context_collapsed.push(err_msg);
    let mut output: String = String::new();

    for (indent, context_line) in context_collapsed.into_iter().enumerate() {
        for _ in 0..indent {
            write!(output, "    ")?;
        }
        writeln!(output, "{}", context_line)?;
    }

    // Remove the trailing newline.
    output.pop();
    write!(f, "{}", output)
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

            fn with_context<T: Into<String>>(mut self, context: T) -> Self {
                self.context.insert(0, context.into());
                self
            }

            fn print_err(self) {
                eprintln!("{}", self);
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
            fn with_context<T: Into<String>>(self, context: T) -> RuntimeError {
                match self {
                    $(
                        RuntimeError::$name(error) => RuntimeError::$name(error.with_context(context.into())),
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

            fn print_err(self) {
                match self {
                    $(
                        RuntimeError::$name(error) => error.print_err(),
                    )*
                }
            }
        }

        impl fmt::Display for RuntimeError {
            fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
                match &self {
                    $(
                        RuntimeError::$name(error) => write!(f, "{}", error),
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

macro_rules! display_error {
    ($name:ident, $header:expr) => {
        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
                let message_lowercase = first_letter_to_lowercase(self.message.clone());
                let err_message = format!($header, message_lowercase);
                format_error_with_context(f, self.context().to_owned(), err_message)
            }
        }
    };
}

runtime_errors!(ArgumentError, InternalError, SystemError);
display_error!(ArgumentError, "Invalid argument: {}");
display_error!(InternalError, "Internal error: {}");
display_error!(SystemError, "System error: {}");

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
    fn with_context<S: Into<String>>(self, context: S) -> Self {
        match self {
            Ok(value) => Ok(value),
            Err(error) => Err(error.with_context(context.into())),
        }
    }

    fn context(&self) -> &[String] {
        match self {
            Ok(_) => &[],
            Err(error) => error.context(),
        }
    }

    fn print_err(self) {
        if let Err(error) = self {
            error.print_err();
        }
    }
}

fn first_letter_to_lowercase(mut string: String) -> String {
    if let Some(first_char) = string.get_mut(0..1) {
        first_char.make_ascii_lowercase();
    }
    string
}
