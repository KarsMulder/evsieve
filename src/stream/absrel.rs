// SPDX-License-Identifier: GPL-2.0-or-later

use std::collections::HashMap;

use crate::key::Key;
use crate::event::{Event, Channel};
use crate::capability::{Capability, Certainty};
use crate::range::{Interval, Set};

pub struct RelToAbs {
    input_key: Key,
    /// The output key must uphold the invariant of being usable as ouput key.
    /// It must in particular not contain any ranges, because trying to merge a
    /// range will panic.
    output_key: Key,
    output_range: Interval,
    speed: f64,
    
    // For each channel that this argument may output, keeps track of the current value it has.
    state: HashMap<Channel, f64>,
}

impl RelToAbs {
    pub fn new(input_key: Key, output_key: Key, output_range: Interval, speed: f64) -> RelToAbs {
        RelToAbs {
            input_key, output_key, output_range, speed,
            state: HashMap::new(),
        }
    }

    fn apply(&mut self, event: Event, output_events: &mut Vec<Event>) {
        // Check if we shoult map this event to something else.
        if self.input_key.matches(&event) {
            let mut output_event = self.output_key.merge(event);

            // Add the input event's value to the current value of the target channel.
            // TODO: figure out initial value.
            let channel_state = self.state.entry(output_event.channel()).or_insert(0.0);
            *channel_state += (event.value as f64) * self.speed;
            *channel_state = self.output_range.bound_f64(*channel_state);
            // Then set the output event's value to that of the channel.
            output_event.value = (*channel_state).floor() as i32;

            return output_events.push(output_event);
        }

        // If this event has the same channel as the target of this --rel-to-abs map, then we
        // overwrite the stored value with the value of this event. In any case, pass the event
        // on as-is.
        if self.output_key.matches_channel(event.channel()) {
            // TODO: Better handling of out-of-range values.
            *self.state.entry(event.channel()).or_default() = event.value as f64;
        }
        output_events.push(event);
    }

    /// Analogue of Map::apply_to_all().
    pub fn apply_to_all(&mut self, events: &[Event], output_events: &mut Vec<Event>) {
        for event in events {
            self.apply(*event, output_events);
        }
    }

    fn apply_to_cap(&self, cap: &Capability, output_caps: &mut Vec<Capability>) {
        // Compute the merged cap, though we are not writing it to the output caps yet.
        

        let (match_certainty, matching_values) = self.input_key.matches_cap(&cap);
        let potentially_nonmatching_values = match match_certainty {
            Certainty::Maybe => cap.values.clone(),
            Certainty::Always => cap.values.setminus(&matching_values),
        };
        let potentially_nonmatching_cap = cap.clone().with_values(potentially_nonmatching_values);
    
        if !matching_values.is_empty() {
            let mut merged_cap = self.output_key.merge_cap(cap.clone());

            // Important: usually merge_cap will return an empty capability if matching_values() is
            // empty. However, since we manually overwrite values here, it is important to check that
            // the matching values are nonempty.
            merged_cap.values = Set::from(self.output_range);
            output_caps.push(merged_cap)
        }

        if !potentially_nonmatching_cap.values.is_empty() {
            output_caps.push(potentially_nonmatching_cap);
        }
    }

    /// Analogue of Map::apply_to_all_caps().
    pub fn apply_to_all_caps(&self, caps: &[Capability], output_caps: &mut Vec<Capability>) {
        for cap in caps {
            self.apply_to_cap(cap, output_caps);
        }
    }
}
