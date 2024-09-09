// SPDX-License-Identifier: GPL-2.0-or-later

pub mod print;
pub mod withhold;
pub mod hook;
pub mod map;
pub mod delay;
pub mod merge;
pub mod absrel;
pub mod scale;
pub mod sink;

use std::collections::HashMap;

use withhold::HookGroup;

use self::absrel::RelToAbs;
use self::map::{Map, Toggle};
use self::hook::Hook;
use self::print::EventPrinter;
use self::scale::Scale;
use self::merge::Merge;

use crate::io::input::InputDevice;
use crate::predevice::PreOutputDevice;
use crate::state::{State, ToggleIndex};
use crate::event::{Event, Namespace};
use crate::capability::{Capability, InputCapabilites};
use crate::io::output::OutputSystem;
use crate::error::RuntimeError;
use crate::loopback::{Loopback, LoopbackHandle, Delay};
use crate::time::Instant;

/// An enum of everything that can be part of the event processing stream.
///
/// There is no formal interface of what these entries need to be capable of, but they need to
/// have rhoughly two functions:
///
/// * `apply_to_all()`, which takes as input a buffer of events, processes them, and then writes
///   them to an output buffer. Events that are left untouched must be written to the output buffer
///   as well, because anything not written to the output buffer is dropped.
/// * `apply_to_all_caps()`, which is like the previous function, but applies to capabilities instead.
///   Given all events (capabilities) that can possibly enter this entry, it must write all
///   events/capabilities that can leave this entry to an output buffer.
/// * `wakeup()`: entries can use the `LoopbackHandle` to request that their `wakeup()` method is
///   called at a laterpoint in time with a certain token. When their `wakeup()` is called, they
///   should check if the token is one of the tokens they scheduled, and if so, do something.
///   It is possible for `wakeup()` to be called with irrelevant tokens, in which case they
///   should do nothing. The `wakeup()` method may output new events for the stream.
///
/// Note that `apply_to_all()` is allowed to take an `&mut self` to change event handling logic at
/// runtime, but it should never modify `self` in a way that the output of `apply_to_all_caps()` changes.
/// The output of `apply_to_all_caps()` must be agnostic of the entry's current runtime state.
pub enum StreamEntry {
    Map(Map),
    Hook(Hook),
    HookGroup(HookGroup),
    Toggle(Toggle),
    Print(EventPrinter),
    Merge(Merge),
    Scale(Scale),
    RelToAbs(RelToAbs),
    Delay(self::delay::Delay),
}

pub struct Setup {
    stream: Vec<StreamEntry>,
    output: OutputSystem,
    state: State,
    toggle_indices: HashMap<String, ToggleIndex>,
    loopback: Loopback,
    /// The capabilities all input devices are capable of, and the tentative capabilites of devices that
    /// may be (re)opened in the future. If a new device gets opened, make sure to call `update_caps`
    /// with that device to keep the bookholding straight.
    input_caps: InputCapabilites,
    /// A vector of events that have been "sent" to an output device but are not actually written
    /// to it yet because we await an EV_SYN event.
    staged_events: Vec<Event>,
}

impl Setup {
    pub fn create(
        stream: Vec<StreamEntry>,
        pre_output: Vec<PreOutputDevice>,
        state: State,
        toggle_indices: HashMap<String, ToggleIndex>,
        input_caps: InputCapabilites,
    ) -> Result<Setup, RuntimeError> {
        let caps_vec: Vec<Capability> = crate::capability::input_caps_to_vec(&input_caps);
        let caps_out = run_caps(&stream, caps_vec);
        let output = OutputSystem::create(pre_output, caps_out)?;
        Ok(Setup {
            stream, output, state, toggle_indices, input_caps,
            loopback: Loopback::new(), staged_events: Vec::new(),
        })
    }

    /// Call this function if the capabilities of a certain input device may have changed, e.g. because
    /// it has been reopened after the program started. If the new capabilities are incompatible with
    /// its previous capabilities, then output devices may be recreated.
    pub fn update_caps(&mut self, new_device: &InputDevice) {
        let old_caps_opt = self.input_caps.insert(
            new_device.domain(),
            new_device.capabilities().clone()
        );

        if let Some(old_caps) = old_caps_opt {
            if new_device.capabilities().is_compatible_with(&old_caps) {
                return;
            }
        }

        let caps_vec: Vec<Capability> = crate::capability::input_caps_to_vec(&self.input_caps);
        let caps_out = run_caps(&self.stream, caps_vec);
        self.output.update_caps(caps_out);
    }

    pub fn time_until_next_wakeup(&self) -> Delay {
        self.loopback.time_until_next_wakeup()
    }

    pub fn toggle_indices(&self) -> &HashMap<String, ToggleIndex> {
        &self.toggle_indices
    }

    pub fn state(&self) -> &State {
        &self.state
    }

    pub fn state_mut(&mut self) -> &mut State {
        &mut self.state
    }
}

/// Handles a single event that was generated by an input device. This is the function other
/// modules are supposed to call when they have an input event they want to get handled.
pub fn run(setup: &mut Setup, time: Instant, event: Event) {
    if event.ev_type().is_syn() {
        syn(setup);
    } else {
        // If the auto-scan feature is enabled, MSC_SCAN events will be automatically
        // generated and are therefore blocked just like EV_SYN events are.
        if cfg!(feature = "auto-scan") {
            if event.code == crate::event::EventCode::MSC_SCAN {
                return;
            }
        }

        let mut loopback_handle = setup.loopback.get_handle(time);
        let mut events_out = Vec::new();

        run_events(
            vec![event],
            &mut events_out,
            &mut setup.stream,
            &mut setup.state,
            &mut loopback_handle,
        );

        // If a single event gets mapped to a single event, then the resulting event gets
        // synchronised whenever the input device does. This makes the result of
        //     --input PATH grab --output
        // generate an output device that resembles the input as closely as possible.
        //
        // However, when a single event gets mapped to multiple events, we want to add a
        // SYN event after each event, because otherwise the OS might misorder commands like
        //     --input PATH grab --map key:f12 key:leftctrl key:c --output
        match events_out.len() {
            0 => {},
            1 => setup.staged_events.extend(events_out),
            _ => {
                for event in events_out {
                    setup.staged_events.push(event);
                    syn(setup);
                }
            }
        }
    }
}

/// Runs all events from the loopback device that were due before `now`. If running such an event causes
/// other events to get added that are due before now, then those events get processed as well.
pub fn wakeup_until(setup: &mut Setup, now: Instant) {
    while let Some((instant, token)) = setup.loopback.poll_once(now) {
        let mut loopback_handle = setup.loopback.get_handle(instant);
        run_wakeup(
            token,
            &mut setup.staged_events,
            &mut setup.stream,
            &mut setup.state,
            &mut loopback_handle,
        );
        
        syn(setup);
    };
}

pub fn syn(setup: &mut Setup) {
    setup.output.route_events(&setup.staged_events);
    setup.staged_events.clear();
    setup.output.synchronize();
}

/// Starts processing the stream at a given starting point.
/// 
/// The usual way to call it is by starting with a single input event as events_in and the
/// streamequal to `Setup.stream`, as done by `run()`. However, the starting point can be
/// changed by passing a subslice of `Setup.stream` as the stream. Furthermore, it can start
/// the stream with multiple events in it, though this shouldn't be done for events that were
/// read from actual input devices. This advanced configurability is mainly intended for the
/// `wakeup()` function to be able to pause and resume event processing at a later point in time.
/// 
/// `stream` may be the empty slice.
fn run_events(events_in: Vec<Event>, events_out: &mut Vec<Event>, stream: &mut [StreamEntry], state: &mut State, loopback: &mut LoopbackHandle) {
    let mut events: Vec<Event> = events_in;
    let mut buffer: Vec<Event> = Vec::new();

    for entry in stream {
        // TODO: (low-priority) Maybe it is time to write a trait with some default implementations
        // for the following almost-copy-pasta?
        match entry {
            StreamEntry::Map(map) => {
                map.apply_to_all(&events, &mut buffer);
                events.clear();
                std::mem::swap(&mut events, &mut buffer);
            },
            StreamEntry::Toggle(toggle) => {
                toggle.apply_to_all(&events, &mut buffer, state);
                events.clear();
                std::mem::swap(&mut events, &mut buffer);
            },
            StreamEntry::Merge(merge) => {
                merge.apply_to_all(&events, &mut buffer);
                events.clear();
                std::mem::swap(&mut events, &mut buffer);
            },
            StreamEntry::RelToAbs(rel_to_abs) => {
                rel_to_abs.apply_to_all(&events, &mut buffer);
                events.clear();
                std::mem::swap(&mut events, &mut buffer);
            },
            StreamEntry::Hook(hook) => {
                hook.apply_to_all(&events, &mut buffer, state, loopback);
                events.clear();
                std::mem::swap(&mut events, &mut buffer);
            },
            StreamEntry::HookGroup(hook_group) => {
                hook_group.apply_to_all(&events, &mut buffer, state, loopback);
                events.clear();
                std::mem::swap(&mut events, &mut buffer);
            },
            StreamEntry::Scale(scale) => {
                scale.apply_to_all(&events, &mut buffer);
                events.clear();
                std::mem::swap(&mut events, &mut buffer);
            },
            StreamEntry::Delay(delay) => {
                delay.apply_to_all(&events, &mut buffer, loopback);
                events.clear();
                std::mem::swap(&mut events, &mut buffer);
            },
            StreamEntry::Print(printer) => {
                printer.apply_to_all(&events);
            },
        }
    }

    events_out.extend(
        events.into_iter().filter(|event| event.namespace == Namespace::Output)
    );
}

fn run_wakeup(token: crate::loopback::Token, events_out: &mut Vec<Event>, stream: &mut [StreamEntry], state: &mut State, loopback: &mut LoopbackHandle) {
    let mut events: Vec<Event> = Vec::new();

    for index in 0 .. stream.len() {
        match &mut stream[index] {
            StreamEntry::Map(_) => {},
            StreamEntry::Toggle(_) => {},
            StreamEntry::Merge(_) => {},
            StreamEntry::Hook(hook) => {
                hook.wakeup(&token);
            },
            StreamEntry::HookGroup(hook_group) => {
                hook_group.wakeup(&token, &mut events);
            },
            StreamEntry::Delay(delay) => {
                delay.wakeup(&token, &mut events);
            },
            StreamEntry::Print(_) => {},
            StreamEntry::Scale(_) => {},
            StreamEntry::RelToAbs(_) => {},
        }

        if ! events.is_empty() {
            // If index+1 == stream.len(), then stream[index+1..] is the empty slice.
            run_events(events, events_out, &mut stream[index+1..], state, loopback);
            events = Vec::new();
        }
    }
}

/// A direct analogue for run_once(), except it runs through capabilities instead of events.
pub fn run_caps(stream: &[StreamEntry], capabilities: Vec<Capability>) -> Vec<Capability> {
    let mut caps: Vec<Capability> = capabilities;
    let mut buffer: Vec<Capability> = Vec::new();
    let mut last_num_caps = caps.len();
    
    for entry in stream {
        match entry {
            StreamEntry::Map(map) => {
                map.apply_to_all_caps(&caps, &mut buffer);
                caps.clear();
                std::mem::swap(&mut caps, &mut buffer);
            },
            StreamEntry::Toggle(toggle) => {
                toggle.apply_to_all_caps(&caps, &mut buffer);
                caps.clear();
                std::mem::swap(&mut caps, &mut buffer);
            },
            StreamEntry::Merge(_) => (),
            StreamEntry::Hook(hook) => {
                hook.apply_to_all_caps(&caps, &mut buffer);
                caps.clear();
                std::mem::swap(&mut caps, &mut buffer);
            },
            StreamEntry::HookGroup(hook_group) => {
                hook_group.apply_to_all_caps(&caps, &mut buffer);
                caps.clear();
                std::mem::swap(&mut caps, &mut buffer);
            },
            StreamEntry::Scale(scale) => {
                scale.apply_to_all_caps(&caps, &mut buffer);
                caps.clear();
                std::mem::swap(&mut caps, &mut buffer);
            },
            StreamEntry::RelToAbs(rel_to_abs) => {
                rel_to_abs.apply_to_all_caps(&caps, &mut buffer);
                caps.clear();
                std::mem::swap(&mut caps, &mut buffer);
            },
            StreamEntry::Print(_) => (),
            StreamEntry::Delay(_) => (),
        }

        // Merge capabilities that differ only in value together when possible.
        // This avoids a worst-case scenario with exponential computation time.
        if caps.len() >= 2 * last_num_caps {
            caps = crate::capability::aggregate_capabilities(caps);
            last_num_caps = caps.len();
        }
    }

    caps.into_iter().filter(|cap| cap.namespace == Namespace::Output).collect()
}