// SPDX-License-Identifier: GPL-2.0-or-later

use std::thread;
use std::process::{Command, Stdio, Child};
use std::io;
use crate::range::Range;
use crate::key::Key;
use crate::state::{State, BoolIndex};
use crate::event::Event;

pub type Effect = Box<dyn Fn(&mut State)>;

/// A tracker is used to track whether a certain key is held down. This is useful for --hook type
/// arguments.
#[derive(Debug)]
struct Tracker {
    key: Key,
    range: Range,
    state_index: BoolIndex,
}

impl Tracker {
    fn new(mut key: Key, state: &mut State) -> Tracker {
        let range = key.pop_value().unwrap_or_else(|| Range::new(Some(1), None));
        Tracker {
            key,
            range,
            state_index: state.push_bool(false),
        }
    }

    /// If the event matches, remembers whether this event falls in the desired range.
    /// If this event falls in the desired range and the previous one didn't, returns true.
    /// Otherwise, returns false.
    fn apply(&self, event: &Event, state: &mut State) -> bool {
        if self.key.matches(event) {
            let previous_value = state[self.state_index];
            let new_value = self.range.contains(event.value);
            state[self.state_index] = self.range.contains(event.value);
            
            new_value && ! previous_value
        } else {
            false
        }
    }

    fn is_down(&self, state: &mut State) -> bool {
        state[self.state_index]
    }
}

pub struct Hook {
    hold_trackers: Vec<Tracker>,
    effects: Vec<Effect>,
}

impl Hook {
    pub fn new(hold_keys: Vec<Key>, state: &mut State) -> Hook {
        let hold_trackers = hold_keys.into_iter().map(
            |key| Tracker::new(key, state)
        ).collect();
        Hook { hold_trackers, effects: Vec::new() }
    }

    pub fn add_effect(&mut self, effect: Effect) {
        self.effects.push(effect);
    }

    fn apply(&self, event: &Event, state: &mut State) {
        let any_tracker_activated = self.hold_trackers.iter().any(
            |tracker| tracker.apply(event, state)
        );

        // Check whether at least one tracker turned active that wasn't on active,
        // i.e. whether this event contributed to the filters of this hook.
        if ! any_tracker_activated {
            return;
        }

        // Test whether all other trackers are active.
        for tracker in &self.hold_trackers {
            if ! tracker.is_down(state) {
                return;
            }
        }
        self.apply_effects(state);
    }

    fn apply_effects(&self, state: &mut State) {
        for effect in &self.effects {
            effect(state);
        }
    }

    pub fn apply_to_all(&self, events: &[Event], state: &mut State) {
        for event in events {
            self.apply(event, state);
        }
    }

    /// Makes this hook invoke an external subprocess when this hook is triggered.
    pub fn add_command(&mut self, program: String, args: Vec<String>) {
        // Compute a printable version of the command, so we have something to show the
        // user in case an error happens.
        let printable_cmd: String = vec![program.clone()].into_iter().chain(args.iter().map(
            |arg| if arg.contains(' ') {
                format!("\"{}\"", arg)
            } else {
                arg.clone()
            }
        )).collect::<Vec<String>>().join(" ");

        self.add_effect(
            Box::new(move |_| {
                let program = program.clone();
                let args = args.clone();
                let printable_cmd = printable_cmd.clone();

                thread::spawn(move || {
                    let child_res: Result<Child, io::Error> =
                        Command::new(program)
                        .args(args)
                        .stdin(Stdio::null())
                        .spawn();
                    let mut child = match child_res {
                        Ok(proc) => proc,
                        Err(error) => {
                            eprintln!("Failed to run {}: {}", printable_cmd, error);
                            return;
                        }
                    };

                    // ISSUE: handling child processes after the evsieve exits
                    let result = child.wait();
                    match result {
                        Err(error) => eprintln!("Error while waiting on {}: {}", printable_cmd, error),
                        Ok(status) => {
                            if ! status.success() {
                                match status.code() {
                                    Some(code) => eprintln!("Failed to run {}: return code {}.", printable_cmd, code),
                                    None => eprintln!("Failed to run {}: interrupted by signal.", printable_cmd),
                                }
                            }
                        }
                    }
                });
            })
        );
    }
}