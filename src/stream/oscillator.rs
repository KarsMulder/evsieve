// SPDX-License-Identifier: GPL-2.0-or-later

use std::collections::HashMap;

use crate::loopback::{LoopbackHandle, Token};
use crate::event::{Channel, Event};
use crate::key::Key;
use crate::time::Duration;

/// While a certain key is held, the key shall appear to turn on and off in the output stream.
pub struct Oscillator {
    /// Only EV_KEY keys that match one of the following keys will be oscillated.
    keys: Vec<Key>,
    /// How long a key will appear to be held down.
    active_time: Duration,
    /// How long a key will appear to be released.
    inactive_time: Duration,

    held_keys: HashMap<Channel, OscillationState>,
}

struct OscillationState {
    /// Whether the last event we sent out makes this key appear active or inactive.
    appears_active: bool,
    /// The token that determines when we will send the next key up/down event.
    next_token: Token,
}

impl Oscillator {
    pub fn new(keys: Vec<Key>, active_time: Duration, inactive_time: Duration) -> Oscillator {
        Oscillator {
            keys, active_time, inactive_time,
            held_keys: HashMap::new(),
        }
    }

    pub fn apply_to_all(&mut self, events: &[Event], output_events: &mut Vec<Event>, loopback: &mut LoopbackHandle) {
        for &event in events {
            self.apply(event, output_events, loopback);
        }
    }

    fn apply(&mut self, event: Event, output_events: &mut Vec<Event>, loopback: &mut LoopbackHandle) {
        // Ignore non-EV_KEY events, which might otherwise match some key such as "@in".
        if ! event.ev_type().is_key() {
            return output_events.push(event);
        }
        if ! self.keys.iter().any(|key| key.matches(&event)) {
            return output_events.push(event);
        }

        let channel = event.channel();

        // If the user releases the key, propagate the key_up event if the key formerly appeared to be
        // held down. If due to oscillation the key was already appearing to be released, drop the event.
        if event.value == 0 {
            let last_state = self.held_keys.remove(&channel);
            if let Some(state) = last_state {
                loopback.cancel_token(state.next_token);
                if state.appears_active {
                    return output_events.push(event);
                }
            }
            return; // Drop event

        } else if event.value == 1 {
            let state_entry = self.held_keys.entry(channel);
            match state_entry {
                // An entry should only exist if the channel was already held down, so we receive a key_down
                // event for a key that was already held down. To maintain the illusion of --oscillate being
                // the source of the key events, drop this duplicate event.
                std::collections::hash_map::Entry::Occupied(_) => {
                    return; // Drop event
                },
                // Otherwise, pass the event on as _the_ event that caused this key to be pressed.
                std::collections::hash_map::Entry::Vacant(vacant_entry) => {
                    vacant_entry.insert(OscillationState {
                        appears_active: true, next_token: loopback.schedule_wakeup_in(self.active_time)
                    });
                    return output_events.push(event);
                },
            }
        } else {
            // We should only have event.value = 2 here if the evdev protocol is followed.
            // If not... well, let's pretend all other event codes are some kind of repeat-event variant.
            if let Some(state) = self.held_keys.get(&channel) {
                if state.appears_active {
                    // If the button appears active, pass all repeat events through as-is.
                    return output_events.push(event);
                }
            }
            // Otherwise, drop all repeat events.
            return;
        }
    }

    /// Activates or deactivates keys that are currently held down and must be oscillated.
    pub fn wakeup(&mut self, token: &Token, output_events: &mut Vec<Event>, loopback: &mut LoopbackHandle) {
        for (channel, state) in &mut self.held_keys {
            if state.next_token == *token {
                let key_must_be_made_active = !state.appears_active;
                let (code, domain) = *channel;
                let event_with_value = |value, previous_value| Event {
                    code, domain, value, previous_value,
                    namespace: crate::event::Namespace::User,
                };

                match key_must_be_made_active {
                    // TODO (HIGH-PRIORITY) Should previous_value match up with the previous value observed by --oscillate?
                    true => {
                        output_events.push(event_with_value(1, 0));
                        state.next_token = loopback.schedule_wakeup_in(self.active_time);
                    },
                    false => {
                        output_events.push(event_with_value(0, 1));
                        state.next_token = loopback.schedule_wakeup_in(self.inactive_time);
                    }
                }

                state.appears_active = key_must_be_made_active;
            }
        }
    }
}