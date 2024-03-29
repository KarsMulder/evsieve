// SPDX-License-Identifier: GPL-2.0-or-later

use std::collections::HashMap;

use crate::key::Key;
use crate::event::{Event, Channel};

/// Represents a --merge argument.
pub struct Merge {
    /// The keys that are subject to getting merged by this argument.
    keys: Vec<Key>,
    
    /// How many down events each (type, code, domain) pair has.
    state: HashMap<Channel, usize>,
}

impl Merge {
    pub fn new(keys: Vec<Key>) -> Merge {
        Merge { keys, state: HashMap::new() }
    }

    #[allow(clippy::needless_return)]
    fn apply(&mut self, event: Event, output_events: &mut Vec<Event>) {
        // If this merge is not applicable to this event, silently pass it on.
        if ! event.ev_type().is_key() || ! self.keys.iter().any(|key| key.matches(&event)) {
            output_events.push(event);
            return;
        }

        let current_down_count: &mut usize = self.state.entry(event.channel()).or_insert(0);
        let last_down_count: usize = *current_down_count;
        match event.value {
            // If this is a KEY_DOWN (1) event, add one to the down count.
            1 => *current_down_count += 1,
            // If this is a KEY_UP (0) event, substract one from the down count, but never go below zero.
            0 => *current_down_count = current_down_count.saturating_sub(1),
            // Otherwise, silently pass on and ignore this event.
            _ => {
                output_events.push(event);
                return;
            },
        }

        match (last_down_count, event.value) {
            // If a KEY_UP event let to the down count becoming zero, or a KEY_DOWN event let to the
            // count becoming one, write it to the output. Importantly, do not pass the event on in
            // case a KEY_UP event resulted into the event staying zero (last: 0, current: 0).
            (1, 0) | (0, 1) => output_events.push(event),
            // Otherwise, drop this event.
            _ => return,
        }
    }

    pub fn apply_to_all(&mut self, events: &[Event], output_events: &mut Vec<Event>) {
        for &event in events {
            self.apply(event, output_events);
        }
    }
}
