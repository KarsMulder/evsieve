// SPDX-License-Identifier: GPL-2.0-or-later

use crate::event::Event;
use std::time::Instant;
use std::num::NonZeroI32;
use std::convert::TryInto;

pub struct LoopbackDevice {
    scheduled_events: Vec<(Instant, Event)>,
}

pub enum NextEventDelay {
    /// The next event should be emitted now, or sometime in the past.
    Now,
    /// No loopback events have been scheduled.
    Never,
    /// Wait a certain amount of milliseconds until the next event is ready.
    Wait(NonZeroI32),
}

impl LoopbackDevice {
    pub fn schedule_event(&mut self, time: Instant, event: Event) {
        self.scheduled_events.push((time, event));
    }

    pub fn time_until_next_event(&self) -> NextEventDelay {
        let next_instant_opt = self.scheduled_events.iter()
            .map(|(time, _event)| time)    
            .min();
        
        // If None, then then there are no events scheduled to happen.
        let next_instant = match next_instant_opt {
            Some(value) => value,
            None => return NextEventDelay::Never,
        };

        // If None, then the event should've been scheduled at some time in the past.
        let duration = match next_instant.checked_duration_since(Instant::now()) {
            Some(value) => value,
            None => return NextEventDelay::Now,
        };

        // If None, then the delay is very, very far in the future. It probably means the user
        // entered some bogus number for the delay. Let's not panic.
        let millisecond_wait: i32 = match duration.as_millis().try_into() {
            Ok(value) => value,
            Err(_) => return NextEventDelay::Never,
        };

        // Ensure that we do not construct a NextEventDelay::Wait(0) result.
        match NonZeroI32::new(millisecond_wait) {
            Some(value) => NextEventDelay::Wait(value),
            None => NextEventDelay::Now,
        }
    }

    /// Returns all ready events, and removes those ready events from self.
    pub fn poll(&mut self) -> Vec<Event> {
        let now = Instant::now();
        let mut ready_events: Vec<Event> = Vec::new();
        let mut unready_events: Vec<(Instant, Event)> = Vec::new();
        for &(time, event) in &self.scheduled_events {
            if time <= now {
                ready_events.push(event);
            } else {
                unready_events.push((time, event));
            }
        }
        self.scheduled_events = unready_events;
        ready_events
    }
}