// SPDX-License-Identifier: GPL-2.0-or-later

use std::collections::HashMap;

use crate::key::Key;
use crate::event::{Event, EventCode};
use crate::domain::Domain;

/// Represents a --merge argument.
pub struct Merge {
    /// The keys that are subject to getting merged by this argument.
    keys: Vec<Key>,
    
    /// How many down events each (type, code, domain) pair has.
    state: HashMap<(EventCode, Domain), isize>,
}

impl Merge {
    pub fn new(keys: Vec<Key>) -> Merge {
        Merge { keys, state: HashMap::new() }
    }

    #[allow(clippy::needless_return)]
    fn apply(&mut self, event: Event, output_events: &mut Vec<Event>) {
        // If this merge is not applicable to this event, silently pass it on.
        if ! self.keys.iter().any(|key| key.matches(&event)) {
            output_events.push(event);
            return;
        }

        let current_down_count: &mut isize = self.state.entry((event.code, event.domain)).or_insert(0);
        match event.value {
            // If this is a KEY_DOWN (1) event, add one to the down count.
            1 => *current_down_count += 1,
            // If this is a KEY_UP (0) event, substract one from the down count.
            0 => *current_down_count -= 1,
            // Otherwise, silently pass on and ignore this event.
            _ => {
                output_events.push(event);
                return;
            },
        }

        // TODO: consider how to deal with keys that were down before the program started.

        match (current_down_count, event.value) {
            // If a KEY_UP event let to the down count becoming zero, or a KEY_DOWN event let to the
            // count becoming one, write it to the output. Importantly, do not pass the event on in
            // case a KEY_UP event resulted into the event count becoming zero.
            (0, 0) | (1, 1) => output_events.push(event),
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
