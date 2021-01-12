// SPDX-License-Identifier: GPL-2.0-or-later

use crate::map::{Map, Toggle};
use crate::hook::Hook;
use crate::state::State;
use crate::event::{Event, Namespace};
use crate::print::EventPrinter;
use crate::capability::Capability;
use crate::io::input::InputSystem;
use crate::io::output::OutputSystem;
use crate::error::RuntimeError;
use crate::ecodes;

pub enum StreamEntry {
    Map(Map),
    Hook(Hook),
    Toggle(Toggle),
    Print(EventPrinter),
}

pub struct Setup {
    stream: Vec<StreamEntry>,
    input: InputSystem,
    output: OutputSystem,
    state: State,
}

impl Setup {
    pub fn new(stream: Vec<StreamEntry>, input: InputSystem, output: OutputSystem, state: State) -> Setup {
        Setup { stream, input, output, state }
    }
}

pub fn run(setup: &mut Setup) -> Result<(), RuntimeError> {
    let input_events = setup.input.poll()?;
    let mut output_events: Vec<Event> = Vec::with_capacity(input_events.len());
    for event in input_events {
        if event.ev_type == ecodes::EV_SYN {
            setup.output.route_events(&output_events);
            output_events.clear();
            setup.output.synchronize();
        } else {
            run_once(event, &mut output_events, &mut setup.stream, &mut setup.state)?;
        }        
    }

    Ok(())
}

pub fn run_once(event_in: Event, events_out: &mut Vec<Event>, stream: &mut [StreamEntry], state: &mut State) -> Result<(), RuntimeError> {
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
            StreamEntry::Hook(hook) => {
                hook.apply_to_all(&events, state);
            },
            StreamEntry::Print(printer) => {
                printer.apply_to_all(&events);
            }
        }
    }

    events_out.extend(
        events.into_iter().filter(|event| event.namespace == Namespace::Output)
    );
    Ok(())
}

/// A direct analogue for run_once, except it runs through capabilities instead of events.
pub fn run_caps(stream: &[StreamEntry], capabilities: Vec<Capability>) -> Vec<Capability> {
    let mut caps: Vec<Capability> = capabilities;
    let mut buffer: Vec<Capability> = Vec::new();
    
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
            StreamEntry::Hook(_) => (),
            StreamEntry::Print(_) => (),
        }

        // Merge capabilities that differ only in value together when possible.
        // This avoids a worst-case scenario with exponential computation time.
        caps = crate::capability::aggregate_capabilities(caps);
    }

    caps.into_iter().filter(|cap| cap.namespace == Namespace::Output).collect()
}