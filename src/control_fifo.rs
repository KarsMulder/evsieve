// SPDX-License-Identifier: GPL-2.0-or-later

use std::os::unix::prelude::AsRawFd;

use crate::error::{SystemError, ArgumentError, Context};
use crate::io::fd::HasFixedFd;
use crate::io::fifo::Fifo;
use crate::arguments::hook::HookToggleAction;
use crate::stream::Setup;
use crate::io::fifo::LineRead;

pub struct ControlFifo {
    source: Box<dyn LineRead>,
    path: String,
}

impl ControlFifo {
    // TODO: Reuse existing Fifo's on the filesystem.
    pub fn create(path: String) -> Result<ControlFifo, SystemError> {
        let source = Box::new(Fifo::create(&path)?);
        Ok(ControlFifo { path, source })
    }

    /// IMPORTANT: this function should never return ArgumentError, because then the fifo would
    /// get closed in case the user provides an incorrect command. Only return SystemError to
    /// signal that something is wrong with the underlying file.
    pub fn poll(&mut self) -> Result<Vec<Command>, SystemError> {
        let lines = self.source.read_lines()?;
        let commands = lines.into_iter()
            .filter(|line| !line.is_empty())
            .filter_map(|line| match parse_command(&line) {
                Ok(effect) => Some(effect),
                Err(error) => {
                    error.with_context(format!("While parsing the command {}:", line)).print_err();
                    None
                }
            }
            ).collect();
        Ok(commands)
    }

    pub fn path(&self) -> &str {
        &self.path
    }
}

pub enum Command {
    Toggle(HookToggleAction),
}

fn parse_command(line: &str) -> Result<Command, ArgumentError> {
    let mut parts = line.split_whitespace();
    let command = match parts.next() {
        Some(command) => command,
        None => return Err(ArgumentError::new("No command provided.")),
    };
    let args: Vec<&str> = parts.collect();

    match command {
        "toggle" => {
            let has_toggle_flag = args.is_empty();
            let toggle_clauses = args.into_iter().map(str::to_owned).collect();
            Ok(Command::Toggle(
                HookToggleAction::parse(has_toggle_flag, toggle_clauses)?
            ))
        },
        _ => Err(ArgumentError::new(format!("Unknown command name: {}", command))),
    }
}

impl Command {
    pub fn execute(self, setup: &mut Setup) -> Result<(), ArgumentError> {
        match self {
            Command::Toggle(action) => {
                // TODO: More helpful error.
                let effects = action.implement(setup.state(), setup.toggle_indices())?;
                for effect in effects {
                    effect(setup.state_mut());
                }
            }
        }

        Ok(())
    }
}

impl AsRawFd for ControlFifo {
    // TODO: Rename path
    fn as_raw_fd(&self) -> std::os::unix::prelude::RawFd {
        self.source.as_raw_fd()
    }
}
unsafe impl HasFixedFd for ControlFifo {}