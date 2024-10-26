// SPDX-License-Identifier: GPL-2.0-or-later

// Allowed because useless default implementations are dead lines of code.
#![allow(clippy::new_without_default)]

// Allowed because assuming that Default::default() returns any specific value seems to go against
// the documentation, which says "Sometimes, you want to fall back to some kind of default value,
// and don’t particularly care what it is."
#![allow(clippy::unwrap_or_default)]

// Allowed because the key "" is a canonically valid key, and comparing a key to "" is more
// idiomatic than asking whether a key is empty.
#![allow(clippy::comparison_to_empty)]

// Allowed because nested ifs allow for more-readable code.
#![allow(clippy::collapsible_if)]

// Disallowed for code uniformity.
#![warn(clippy::explicit_iter_loop)]
#![warn(clippy::explicit_into_iter_loop)]

pub mod event;
pub mod key;
pub mod domain;
pub mod state;
pub mod signal;
pub mod error;
pub mod capability;
pub mod affine;
pub mod range;
pub mod ecodes;
pub mod predevice;
pub mod subprocess;
pub mod daemon;
pub mod loopback;
pub mod stream;
pub mod control_fifo;
pub mod time;
pub mod utils;

#[cfg(feature = "auto-scan")]
pub mod scancodes;

#[cfg(test)]
pub mod tests;

pub mod io {
    pub mod input;
    pub mod epoll;
    pub mod output;
    pub mod internal_pipe;
    pub mod fd;
    pub mod fifo;
}

pub mod persist {
    pub mod inotify;
    pub mod blueprint;
    pub mod subsystem;
    pub mod interface;
    pub mod format;
    pub mod storage;
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
    pub mod merge;
    pub mod delay;
    pub mod withhold;
    pub mod absrel;
    pub mod control_fifo;
    pub mod test;
    pub mod config;
    pub mod scale;
}

pub mod bindings {
    #[allow(warnings)]
    pub mod libevdev;
}

pub mod data {
    pub mod hid_usage;
    pub mod hid_usage_parser;
}

#[macro_use]
extern crate lazy_static;

use std::os::unix::prelude::{AsRawFd, RawFd};

use arguments::parser::Implementation;
use error::{RuntimeError, Context};
use io::epoll::{Epoll, FileIndex, Message};
use io::fd::HasFixedFd;
use io::input::InputDevice;
use io::output::UInputSystem;
use persist::interface::HostInterfaceState;
use stream::Setup;
use signal::{SigMask, SignalFd};
use control_fifo::{CommandInfo, ControlFifo};

use crate::error::SystemError;
use crate::event::EventCode;
use crate::persist::subsystem::Report;
use crate::predevice::PersistState;


fn main() {
    let result = run_and_interpret_exit_code();
    daemon::await_completion();
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

pub enum Pollable {
    InputDevice(InputDevice),
    SignalFd(SignalFd),
    ControlFifo(ControlFifo),
    PersistSubsystem(persist::interface::HostInterface),
}
unsafe impl HasFixedFd for Pollable {}

impl AsRawFd for Pollable {
    fn as_raw_fd(&self) -> RawFd {
        match self {
            Pollable::InputDevice(device) => device.as_raw_fd(),
            Pollable::SignalFd(fd) => fd.as_raw_fd(),
            Pollable::ControlFifo(fifo) => fifo.as_raw_fd(),
            Pollable::PersistSubsystem(interface) => interface.as_raw_fd(),
        }
    }
}

struct Program {
    epoll: Epoll<Pollable>,
    setup: Setup<UInputSystem>,
    persist_subsystem: HostInterfaceState,
}

const TERMINATION_SIGNALS: [libc::c_int; 3] = [libc::SIGTERM, libc::SIGINT, libc::SIGHUP];

fn run() -> Result<(), RuntimeError> {
    // Check if the arguments contain --help or --version.
    let args: Vec<String> = std::env::args().skip(1).collect();
    if arguments::parser::check_help_and_version(&args) {
        daemon::notify_ready_async();
        return Ok(());
    }

    // Listen for signals sent to this program.
    let mut sigmask = SigMask::new();
    sigmask.add(libc::SIGPIPE);
    for &signal in &TERMINATION_SIGNALS {
        sigmask.add(signal);
    }
    let signal_fd = signal::SignalFd::new(&sigmask)?;
    let mut epoll = Epoll::new()?;
    epoll.add_file(Pollable::SignalFd(signal_fd))?;

    // Additionally block SIGCHLD because another thread listens for it.
    sigmask.add(libc::SIGCHLD);
    let _signal_block = unsafe { signal::SignalBlock::new(&sigmask)? };

    // Parse the arguments and set up the input/output devices.
    let pre_implementation = arguments::parser::process(args)?;
    let Implementation { setup, input_devices, blueprints, control_fifos } = arguments::parser::implement(pre_implementation)?;

    for device in input_devices {
        epoll.add_file(Pollable::InputDevice(device))?;
    }
    for fifo in control_fifos {
        epoll.add_file(Pollable::ControlFifo(fifo))?;
    }

    // If the persistence subsystem is running, this shall keep track of its index in the epoll.
    let mut persist_subsystem: HostInterfaceState = HostInterfaceState::new();

    // If we were given any blueprints, we must launch the persitence subsystem right now and declare
    // that we want those blueprints to be opened.
    if ! blueprints.is_empty() {
        let interface = match persist_subsystem.require(&mut epoll) {
            Some(interface) => interface,
            None => return Err(SystemError::new("Failed to launch the persistence subsystem, which is required to open the input devices flagged with \"persist\".").into()),
        };
        for blueprint in blueprints {
            interface.add_blueprint(blueprint)
                .with_context("While trying to register a perstent device to be opened later")?
        }
    }

    let mut program = Program {
        epoll, setup, persist_subsystem
    };

    daemon::notify_ready_async();

    // Make sure evsieve has something to do.
    if has_no_activity(&program.epoll) {
        println!("Warning: no input devices available. Evsieve will exit now.");
        return Ok(());
    }

    // Iterate over messages generated by the epoll.
    enter_main_loop(&mut program)?;

    // Shut down the persistence system properly.
    program.persist_subsystem.await_shutdown(&mut program.epoll);

    Ok(())
}

/// An enum used to signal to the main loop which action should be taken: if a function returns
/// Action::Continue, the program should go on, otherwise it should perform a clean exit.
enum Action {
    Continue,
    Exit,
}

/// The main loop of the program. Polls the epoll and handles it responses. Quits if an `Action::Exit`
/// is returned by `handle_ready_file()` or `handle_broken_file()`.
fn enter_main_loop(program: &mut Program) -> Result<(), RuntimeError> {
    loop {
        let timeout: i32 = match program.setup.time_until_next_wakeup() {
            loopback::Delay::Now => {
                program.setup.wakeup_until(crate::time::Instant::now());
                continue;
            },
            loopback::Delay::Never => crate::io::epoll::INDEFINITE_TIMEOUT,
            loopback::Delay::Wait(time) => time.get(),
        };

        let messages = program.epoll.poll(timeout).with_context("While polling the epoll for events:")?;

        for message in messages {
            let action = match message {
                Message::Ready(index) => {
                    match handle_ready_file(program, index) {
                        Ok(action) => action,
                        Err(error) => {
                            error.print_err();
                            handle_broken_file(program, index)
                        }
                    }
                },
                Message::Broken(index) => {
                    handle_broken_file(program, index)
                },
                Message::Hup(index) => {
                    match program.epoll.get(index) {
                        Some(Pollable::ControlFifo(_)) => {
                            // HUP for a control FIFO should never happen because we keep the FIFO open
                            // for writing ourselves in order to prevent HUP's from happening. If a HUP
                            // happens anyway, I suppose something is really wrong.
                            eprintln!("Warning: unexpected EPOLLHUP received on a control FIFO.");
                            handle_broken_file(program, index)
                        },
                        _ => handle_broken_file(program, index),
                    }
                },
            };

            match action {
                Action::Continue => continue,
                Action::Exit => return Ok(()),
            }
        }
    }
}

/// If this function returns Err, then `handle_broken_file` needs to be called with the same index.
/// IMPORTANT: this function should NOT return Err if the device at `index` itself is not broken.
/// If some other error occurs, you should handle it in this function itself and then return Ok.
fn handle_ready_file(program: &mut Program, index: FileIndex) -> Result<Action, RuntimeError> {
    let file = match program.epoll.get_mut(index) {
        Some(file) => file,
        None => {
            eprintln!("Internal error: an epoll reported ready on a device that is not registered with it. This is a bug.");
            return Ok(Action::Continue);
        },
    };
    match file {
        Pollable::InputDevice(device) => {
            let events = device.poll().with_context_of(||
                format!("While polling the input device {}:", device.path().display())
            )?;
            for (time, event) in events {
                program.setup.wakeup_until(time);
                program.setup.run(time, event);
            }
            Ok(Action::Continue)
        },
        Pollable::SignalFd(fd) => {
            let siginfo = fd.read_raw()?;
            let signal_no = siginfo.ssi_signo as i32;
            if TERMINATION_SIGNALS.contains(&signal_no) {
                Ok(Action::Exit)
            } else {
                // Ignore other signals, including SIGPIPE.
                Ok(Action::Continue)
            }
        },
        Pollable::ControlFifo(fifo) => {
            let commands = fifo.poll().with_context_of(
                || format!("While polling commands from {}:", fifo.path()),
            )?;
            for command in commands {
                let CommandInfo { original_line, action } = command;

                action.execute(&mut program.setup)
                    .with_context_of(|| format!("While executing the command \"{}\":", original_line))
                    .print_err();
            }

            Ok(Action::Continue)
        },
        Pollable::PersistSubsystem(ref mut interface) => {
            let report = interface.recv().with_context("While polling the persistence subsystem from the main thread:")?;
            Ok(handle_persist_subsystem_report(program, index, report))
        },
    }
}

fn handle_broken_file(program: &mut Program, index: FileIndex) -> Action {
    let broken_device = match program.epoll.remove(index) {
        Some(file) => file,
        None => {
            eprintln!("Internal error: epoll reported a file as broken despite that file not being registered with said epoll.");
            return Action::Continue;
        }
    };
    match broken_device {
        Pollable::InputDevice(mut device) => {
            eprintln!("The device {} has been disconnected.", device.path().display());

            // Release all keys that this device had pressed, so we don't end up with a key stuck on
            // an output device.
            let pressed_keys: Vec<EventCode> = device.get_pressed_keys().collect();
            let now = crate::time::Instant::now();

            for key_code in pressed_keys {
                let release_event = device.synthesize_event(key_code, 0);
                program.setup.run(now, release_event);
            }
            program.setup.syn();

            match device.persist_state() {
                // Mode None: drop the device and carry on without it, if possible.
                PersistState::None => {},
                // Mode Exit: quit evsieve now.
                PersistState::Exit => {
                    return Action::Exit;
                },
                // Mode Reopen: try to reopen the device if it becomes available again later.
                PersistState::Reopen | PersistState::Full(_) => {
                    if let Some(interface) = program.persist_subsystem.require(&mut program.epoll) {
                        interface.add_blueprint(device.into_blueprint())
                            .with_context("While trying to register a disconnected device for reopening:")
                            .print_err()
                    } else {
                        eprintln!("Internal error: cannot reopen device: persistence subsystem not available.")
                    }
                }
            };
        },
        Pollable::ControlFifo(fifo) => {
            eprintln!("Error: the FIFO at {} is no longer available.", fifo.path());
        },
        Pollable::SignalFd(_fd) => {
            eprintln!("Fatal error: signal file descriptor broken.");
            return Action::Exit;
        },
        Pollable::PersistSubsystem(mut interface) => {
            eprintln!("Internal error: the persistence subsystem has broken. Evsieve may fail to open devices specified with the persist flag.");
            let _ = interface.request_shutdown();
            program.persist_subsystem.mark_as_broken();
        },
    }

    if has_no_activity(&program.epoll) {
        println!("No devices to poll events from. Evsieve will exit now.");
        Action::Exit
    } else {
        Action::Continue
    }
}

fn handle_persist_subsystem_report(program: &mut Program, index: FileIndex, report: Report) -> Action {
    match report {
        Report::Shutdown => {
            let _ = program.epoll.remove(index);
            program.persist_subsystem.mark_as_shutdown();
            Action::Continue
        },
        Report::BlueprintDropped => {
            if has_no_activity(&program.epoll) {
                println!("No devices remaining that can possibly generate events. Evsieve will exit now.");
                Action::Exit
            } else {
                Action::Continue
            }
        },
        Report::DeviceOpened(mut device) => {
            if let Err(error) = device.grab_if_desired() {
                error.with_context(format!("While grabbing the device {}:", device.path().display()))
                    .print_err();
                eprintln!("Warning: unable to reopen device {}. The device is most likely grabbed by another program.", device.path().display());
                return Action::Continue
            }

            let device_path = device.path().to_owned();
            program.setup.update_caps(&device);

            match program.epoll.add_file(Pollable::InputDevice(device))
            {
                Ok(_) => println!("The device {} has been reconnected.", device_path.display()),
                Err(error) => {
                    error.with_context("While adding a newly opened device to the epoll:").print_err();
                },
            }

            Action::Continue
        }
    }
}

/// Returns true if evsieve has nothing to do and should just exit.
fn has_no_activity(epoll: &Epoll<Pollable>) -> bool {
    for file in epoll.files() {
        match file {
            Pollable::InputDevice(_) => return false,
            Pollable::PersistSubsystem(_) => return false,
            Pollable::ControlFifo(_) => return false,
            Pollable::SignalFd(_) => (),
        }
    }
    true
}
