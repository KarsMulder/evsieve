// SPDX-License-Identifier: GPL-2.0-or-later

use crate::io::input::InputDevice;
use crate::map::{Map, Toggle};
use crate::hook::Hook;
use crate::merge::Merge;
use crate::predevice::PreOutputDevice;
use crate::state::State;
use crate::event::{Event, Namespace};
use crate::print::EventPrinter;
use crate::capability::{Capability, InputCapabilites};
use crate::io::output::OutputSystem;
use crate::error::RuntimeError;
use crate::loopback::{Loopback, Delay};
use std::time::Instant;

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
///
/// Note that `apply_to_all()` is allowed to take an `&mut self` to change event handling logic at
/// runtime, but it should never modify `self` in a way that the output of `apply_to_all_caps()` changes.
/// The output of `apply_to_all_caps()` must be agnostic of the entry's current runtime state.
/// 
/// TODO: Update doccomment to account for loopback.
pub enum StreamEntry {
    Map(Map),
    Hook(Hook),
    Toggle(Toggle),
    Print(EventPrinter),
    Merge(Merge),
}

pub struct Setup {
    stream: Vec<StreamEntry>,
    output: OutputSystem,
    state: State,
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
        input_caps: InputCapabilites,
    ) -> Result<Setup, RuntimeError> {
        let caps_vec: Vec<Capability> = crate::capability::input_caps_to_vec(&input_caps);
        let caps_out = run_caps(&stream, caps_vec);
        let output = OutputSystem::create(pre_output, caps_out)?;
        Ok(Setup {
            stream, output, state, input_caps,
            loopback: Loopback::new(), staged_events: Vec::new(),
        })
    }

    /// Call this function if the capabilities of a certain input device may have changes, e.g. because
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
}

pub fn run(setup: &mut Setup, event: Event) {
    if event.ev_type().is_syn() {
        syn(setup);
    } else {
        run_event(
            event,
            &mut setup.staged_events,
            &mut setup.stream,
            &mut setup.state,
            &mut setup.loopback,
        );
    }
}

pub fn wakeup(setup: &mut Setup) {
    let wakeup_instants = setup.loopback.poll();
    for instant in wakeup_instants {
        run_wakeup(
            instant,
            &mut setup.staged_events,
            &mut setup.stream,
            &mut setup.state,
            &mut setup.loopback,
        );
    };
    // TODO: consider the pooling behaviour for events with the same instant.
    syn(setup);
}

pub fn syn(setup: &mut Setup) {
    setup.output.route_events(&setup.staged_events);
    setup.staged_events.clear();
    setup.output.synchronize();
}

fn run_event(event_in: Event, events_out: &mut Vec<Event>, stream: &mut [StreamEntry], state: &mut State, loopback: &mut Loopback) {
    let mut events: Vec<Event> = vec![event_in];
    let mut buffer: Vec<Event> = Vec::new();

    for entry in stream {
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
            StreamEntry::Hook(hook) => {
                hook.apply_to_all(&events, &mut buffer, state, loopback);
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

fn run_wakeup(instant: Instant, events_out: &mut Vec<Event>, stream: &mut [StreamEntry], state: &mut State, loopback: &mut Loopback) {
    let mut events: Vec<Event> = Vec::new();

    for index in 0 .. stream.len() {
        match &mut stream[index] {
            StreamEntry::Map(_map) => {},
            StreamEntry::Toggle(_toggle) => {},
            StreamEntry::Merge(_merge) => {},
            StreamEntry::Hook(hook) => {
                hook.wakeup(instant, &mut events);
            },
            StreamEntry::Print(_printer) => {},
        }

        for event in events.drain(..) {
            // TODO: check panic-safety
            run_event(event, events_out, &mut stream[index+1..], state, loopback);
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
            StreamEntry::Print(_) => (),
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