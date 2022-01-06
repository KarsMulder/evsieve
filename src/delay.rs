// SPDX-License-Identifier: GPL-2.0-or-later

use crate::loopback::Loopback;
use crate::event::Event;
use crate::key::Key;
use std::time::{Duration, Instant};

/// All events that reach the delay shall be removed and put back into the stream after 
/// a certain amount of time passes.
pub struct Delay {
    keys: Vec<Key>,
    period: Duration,

    /// State: modifiable at runtime.
    delayed_events: Vec<(Instant, Vec<Event>)>,
}

impl Delay {
    pub fn new(keys: Vec<Key>, period: Duration) -> Delay {
        Delay {
            keys, period,
            delayed_events: Vec::new(),
        }
    }

    /// Checks if some events matches this delay's keys, and if so, witholds them for a
    /// specified period.
    pub fn apply_to_all(&mut self, events: &[Event], output_events: &mut Vec<Event>, loopback: &mut Loopback) {
        let mut events_to_withold: Vec<Event> = Vec::new();
        for &event in events {
            if self.keys.iter().any(|key| key.matches(&event)) {
                events_to_withold.push(event);
            } else {
                output_events.push(event);
            }
        }

        if ! events_to_withold.is_empty() {
            let wakeup_time = Instant::now() + self.period;
            self.delayed_events.push((wakeup_time, events_to_withold));
            loopback.schedule_wakeup(wakeup_time);
        }
    }

    /// All delayed events that are overdue will be put back into the stream.
    pub fn wakeup(&mut self, now: Instant, output_events: &mut Vec<Event>) {
        // Issue: this can merge multiple batches of events together.
        while let Some((wakeup_time, delayed_events)) = self.delayed_events.first() {
            if wakeup_time <= &now {
                output_events.extend(delayed_events);
                self.delayed_events.remove(0);
            } else {
                break;
            }
        }
    }
}