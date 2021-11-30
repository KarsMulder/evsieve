// SPDX-License-Identifier: GPL-2.0-or-later

use crate::key::Key;
use crate::event::Event;
use crate::capability::{Capability, CapMatch};
use crate::range::Range;

// TODO: consider whether this should be a special case of a Map.

pub struct AbsToRel {
    input_key: Key,
    output_key: Key,
    reset_keys: Vec<Key>,

    // The following parameters are stateful.
    /// The amount of movement that has been made but not been written to the output yet, for example
    /// because of fuzz or rounding errors.
    _residual: f64,
    /// If true, then the next ABS_X event received will not cause an EV_REL event to be generated.
    /// This is handy if the user lifts his finger/pen/whatever off the surface and places it elsewhere.
    reset: bool,
}

impl AbsToRel {
    pub fn new(input_key: Key, output_key: Key, reset_keys: Vec<Key>) -> Self {
        Self {
            input_key,
            output_key,
            reset_keys,
            _residual: 0.0,
            reset: true,
        }
    }

    fn apply(&mut self, event_in: Event, output_events: &mut Vec<Event>) {
        if self.reset_keys.iter().any(|key| key.matches(&event_in)) {
            self.reset = true;
            // Intentionally do not return here.
        }

        if ! self.input_key.matches(&event_in) {
            output_events.push(event_in);
            return;
        }
        if self.reset {
            self.reset = false;
            return;
        }

        let mut event_out = self.output_key.merge(event_in);
        event_out.value = event_in.value.saturating_sub(event_in.previous_value);
        output_events.push(event_out);
    }

    pub fn apply_to_all(&mut self, events: &[Event], output_events: &mut Vec<Event>) {
        for &event in events {
            self.apply(event, output_events);
        }
    }

    /// An analogue for apply() but with capabilities instead of events.
    fn apply_cap(&self, cap: Capability, output_caps: &mut Vec<Capability>) {
        let matches_cap = self.input_key.matches_cap(&cap);

        // An iterator of the caps we would add if we matched. Do not actually add them yet.
        let mut generated_cap = self.output_key.merge_cap(cap);
        // TODO: fix incorrect calculation.
        generated_cap.value_range = Range::new(None, None);
        
        // Depending on whether or not we match, we should add the generated capabilities
        // and preserve/remove self from the stream.
        match matches_cap {
            CapMatch::Yes => {
                output_caps.push(generated_cap);
            },
            CapMatch::Maybe => {
                output_caps.push(cap);
                output_caps.push(generated_cap);
            },
            CapMatch::No => {
                output_caps.push(cap);
            },
        }
    }

    /// Like apply_to_all(), but for capabilities.
    pub fn apply_to_all_caps(&self, caps: &[Capability], output_caps: &mut Vec<Capability>) {
        for &cap in caps {
            self.apply_cap(cap, output_caps);
        }
    }
}