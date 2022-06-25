use std::os::unix::prelude::AsRawFd;

use crate::error::{SystemError, ArgumentError, Context};
use crate::io::fd::HasFixedFd;
use crate::io::fifo::Fifo;
use crate::arguments::hook::HookToggleAction;

pub struct ControlFifo {
    fifo: Fifo,
}

impl ControlFifo {
    // TODO: Reuse existing Fifo's on the filesystem.
    pub fn create(path: &str) -> Result<ControlFifo, SystemError> {
        Ok(ControlFifo {
            fifo: Fifo::create(path)?
        })
    }

    pub fn poll(&mut self) -> Result<Vec<Command>, SystemError> {
        let lines = self.fifo.read_lines()?;
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
            let has_toggle_flag = ! args.is_empty();
            let toggle_clauses = args.into_iter().map(str::to_owned).collect();
            Ok(Command::Toggle(
                HookToggleAction::parse(has_toggle_flag, toggle_clauses)?
            ))
        },
        _ => Err(ArgumentError::new(format!("Unknow command received: {}", command))),
    }
}

impl AsRawFd for ControlFifo {
    // TODO: Rename path
    fn as_raw_fd(&self) -> std::os::unix::prelude::RawFd {
        self.fifo.as_raw_fd()
    }
}
unsafe impl HasFixedFd for ControlFifo {}