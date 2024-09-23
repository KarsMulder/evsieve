// SPDX-License-Identifier: GPL-2.0-or-later

use crate::key::Key;
use crate::event::{Event, Namespace};
use crate::domain::Domain;
use crate::capability::{Capability, Certainty};
use crate::error::InternalError;
use crate::state::{State, ToggleIndex};

#[derive(Clone, Debug)]
pub struct Map {
    input_key: Key,
    output_keys: Vec<Key>,
}

impl Map {
    pub fn new(input_key: Key, output_keys: Vec<Key>) -> Map {
        Map { input_key, output_keys }
    }

    /// Returns a map that blocks a given input key.
    pub fn block(input_key: Key) -> Map {
        Map::new(input_key, Vec::new())
    }

    pub fn domain_shift(
            source_domain: Domain, source_namespace: Namespace,
            target_domain: Domain, target_namespace: Namespace
    ) -> Map {
        Map::new(
            Key::from_domain_and_namespace(source_domain, source_namespace),
            vec![Key::from_domain_and_namespace(target_domain, target_namespace)]
        )
    }

    /// Checks if an event matches this map, and if so, generates corresponding events and
    /// writes those to the output. Otherwise, writes the event itself to the output.
    fn apply(&self, event: Event, output_events: &mut Vec<Event>) {
        if ! self.input_key.matches(&event) {
            output_events.push(event);
            return;
        }
        let generated_events = self.output_keys.iter().map(
            |key| key.merge(event)
        );
        output_events.extend(generated_events);
    }

    /// Maps all events to output_events. Events that do not match this Map are mapped to themselfe.
    /// Preserves the order of the events.
    pub fn apply_to_all(&self, events: &[Event], output_events: &mut Vec<Event>) {
        for &event in events {
            self.apply(event, output_events);
        }
    }

    /// An analogue for apply() but with capabilities instead of events.
    fn apply_cap(&self, cap: Capability, output_caps: &mut Vec<Capability>) {
        let (match_certainty, matching_values) = self.input_key.matches_cap(&cap);

        // The capability of the event values that might match the filter key.
        let matchable_cap = cap.clone().with_values(matching_values);

        // Create the capability with all values that might not match the filter key.
        let potentially_nonmatching_values = cap.values.setminus(&matchable_cap.values);
        let potentially_nonmatching_cap = match match_certainty {
            Certainty::Maybe => cap,
            Certainty::Always => cap.with_values(potentially_nonmatching_values),
        };

        // An iterator of the caps we would add if we matched.
        let generated_caps = self.output_keys.iter().map(
            |key| key.merge_cap(matchable_cap.clone())
        ).filter(|cap| !cap.values.is_empty());
        
        // Add all potential capabilities to the output capabilities vector.
        if !potentially_nonmatching_cap.values.is_empty() {
            output_caps.push(potentially_nonmatching_cap);
        }
        output_caps.extend(generated_caps);
    }

    /// Like apply_to_all(), but for capabilities.
    /// TODO (Low Priority): consider making caps a `impl IntoIterator<Item=Capability>` and calling it with caps.drain(..)
    pub fn apply_to_all_caps(&self, caps: &[Capability], output_caps: &mut Vec<Capability>) {
        for cap in caps {
            self.apply_cap(cap.clone(), output_caps);
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ToggleMode {
    Passive,
    Consistent,
}

/// Represents a --toggle argument.
pub struct Toggle {
    input_key: Key,
    output_keys: Vec<Key>,
    pub mode: ToggleMode,
    pub state_index: ToggleIndex,
}

impl Toggle {
    /// Requires at least one output key.
    /// If a predetermined index is supplied, this state will toggle along with that index.
    /// Otherwise, a new index will be created.
    pub fn new(input_key: Key, output_keys: Vec<Key>, mode: ToggleMode,
            state: &mut State, predetermined_index: Option<ToggleIndex>) -> Result<Toggle, InternalError> {
        let num_output = output_keys.len();
        let state_index = match predetermined_index {
            Some(index) => {
                if state[index].size() == num_output {
                    index
                } else {
                    return Err(InternalError::new("The toggle's index size does not match up with the toggle."))
                }
            },
            None => {
                state.create_toggle_with_size(num_output)?
            },
        };
        
        Ok(Toggle { input_key, output_keys, mode, state_index })
    }

    /// Returns the active output key. Specific events may use a different active output key
    /// than this one. Use active_output_key_for_event() instead.
    fn active_output_key(&self, state: &State) -> &Key {
        &self.output_keys[state[self.state_index].value()]
    }

    /// Returns the currently active key for a specific event.
    /// Identical to active_output_key() for passive maps.
    fn active_output_key_for_event(&self, event: Event, state: &State) -> &Key {
        match self.mode {
            ToggleMode::Passive => self.active_output_key(state),
            ToggleMode::Consistent => {
                match state[self.state_index].memory.get(&event.channel()) {
                    Some(&index) => &self.output_keys[index],
                    None => self.active_output_key(state),
                }
            }
        }
    }

    /// If this is a consistent map, keeps track of which events were last routed where,
    /// to ensure that a key_up event is sent to the same target as a key_down event even
    /// if the active map was toggled in the meantime.
    /// 
    /// This should be called _after_ active_output_key_for_event(), because otherwise it
    /// may erase the memory we were left by the previous event.
    fn remember(&self, event: Event, state: &mut State) {
        if self.mode == ToggleMode::Consistent && event.ev_type().is_key() && self.input_key.matches(&event) {
            let active_value = state[self.state_index].value();
            let memory = &mut state[self.state_index].memory;
            let event_channel = event.channel();
            match event.value {
                0 => { memory.remove(&event_channel); },
                _ => { memory.entry(event_channel).or_insert(active_value); },
            }
        }
    }

    fn apply(&self, event: Event, output_events: &mut Vec<Event>, state: &mut State) {
        if self.input_key.matches(&event) {
            let active_output = self.active_output_key_for_event(event, state);
            self.remember(event, state);
            output_events.push(active_output.merge(event));
        } else {
            output_events.push(event);
        }
    }

    /// The apply_ functions are analogous to the Map::apply_ equivalents.
    pub fn apply_to_all(&self, events: &[Event], output_events: &mut Vec<Event>, state: &mut State) {
        for &event in events {
            self.apply(event, output_events, state);
        }
    }

    pub fn apply_to_all_caps(&self, caps: &[Capability], output_caps: &mut Vec<Capability>) {
        let self_as_map = Map::new(self.input_key.clone(), self.output_keys.clone());
        self_as_map.apply_to_all_caps(caps, output_caps);
    }
}
