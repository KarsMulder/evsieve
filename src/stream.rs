// SPDX-License-Identifier: GPL-2.0-or-later

use crate::map::{Map, Toggle};
use crate::hook::Hook;
use crate::merge::Merge;
use crate::predevice::PreOutputDevice;
use crate::state::State;
use crate::event::{Event, Namespace};
use crate::print::EventPrinter;
use crate::capability::Capability;
use crate::io::output::OutputSystem;
use crate::error::RuntimeError;

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
    /// A vector of events that have been "sent" to an output device but are not actually written
    /// to it yet because we await an EV_SYN event.
    staged_events: Vec<Event>,
}

impl Setup {
    pub fn create(
        stream: Vec<StreamEntry>,
        pre_output: Vec<PreOutputDevice>,
        state: State,
        capabilities_in: Vec<Capability>,
    ) -> Result<Setup, RuntimeError> {
        let capabilities_out = run_caps(&stream, capabilities_in);
        let output = OutputSystem::create(pre_output, capabilities_out)?;
        Ok(Setup { stream, output, state, staged_events: Vec::new() })
    }
}

pub fn run(setup: &mut Setup, event: Event) {
    if event.ev_type().is_syn() {
        setup.output.route_events(&setup.staged_events);
        setup.staged_events.clear();
        setup.output.synchronize();
    } else {
        run_event(event, &mut setup.staged_events, &mut setup.stream, &mut setup.state);
    }
}

pub fn run_event(event_in: Event, events_out: &mut Vec<Event>, stream: &mut [StreamEntry], state: &mut State) {
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
                hook.apply_to_all(&events, state);
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
            StreamEntry::Hook(_) => (),
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