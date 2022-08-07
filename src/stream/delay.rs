// SPDX-License-Identifier: GPL-2.0-or-later

use crate::loopback::{LoopbackHandle, Token};
use crate::event::Event;
use crate::key::Key;
use crate::time::Duration;

// TODO: it appears there is a function libevdev_set_clock_id() which can be used to make
// event devices report their event times on a monotonic clock. This may be useful to
// solve the time-synchronisation issue. Investigate.

/// All events that reach the delay shall be removed and put back into the stream after 
/// a certain amount of time passes.
pub struct Delay {
    keys: Vec<Key>,
    period: Duration,

    /// State: modifiable at runtime.
    /// Events that need to be put back into thes stream when the loopback releases a certain token.
    delayed_events: Vec<(Token, Vec<Event>)>,
}

impl Delay {
    pub fn new(keys: Vec<Key>, period: Duration) -> Delay {
        Delay {
            keys, period,
            delayed_events: Vec::new(),
        }
    }

    /// Checks if some events matches this delay's keys, and if so, withholds them for a
    /// specified period.
    pub fn apply_to_all(&mut self, events: &[Event], output_events: &mut Vec<Event>, loopback: &mut LoopbackHandle) {
        let mut events_to_withhold: Vec<Event> = Vec::new();
        for &event in events {
            if self.keys.iter().any(|key| key.matches(&event)) {
                events_to_withhold.push(event);
            } else {
                output_events.push(event);
            }
        }

        if ! events_to_withhold.is_empty() {
            let wakeup_token = loopback.schedule_wakeup_in(self.period);
            self.delayed_events.push((wakeup_token, events_to_withhold));
        }
    }

    /// All delayed events that are overdue will be put back into the stream.
    pub fn wakeup(&mut self, token: &Token, output_events: &mut Vec<Event>) {
        while let Some((wakeup_token, delayed_events)) = self.delayed_events.first() {
            if wakeup_token == token {
                output_events.extend(delayed_events);
                self.delayed_events.remove(0);
            } else {
                break;
            }
        }
    }
}