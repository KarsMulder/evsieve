// SPDX-License-Identifier: GPL-2.0-or-later

use std::time::{Instant};
use std::num::NonZeroI32;
use std::convert::TryInto;

pub struct Loopback {
    schedule: Vec<Instant>,
}

pub enum Delay {
    Now,
    Never,
    /// Wait a specified amount of milliseconds.
    Wait(NonZeroI32),
}

impl Loopback {
    pub fn new() -> Loopback {
        Loopback {
            schedule: Vec::new(),
        }
    }
    pub fn schedule_wakeup(&mut self, time: Instant) {
        self.schedule.push(time);
    }

    pub fn time_until_next_wakeup(&self) -> Delay {
        let next_instant_opt = self.schedule.iter().min();
        
        // If None, then then there are no events scheduled to happen.
        let next_instant = match next_instant_opt {
            Some(value) => value,
            None => return Delay::Never,
        };

        // If None, then the event should've been scheduled at some time in the past.
        let duration = match next_instant.checked_duration_since(Instant::now()) {
            Some(value) => value,
            None => return Delay::Now,
        };

        // If None, then the delay is very, very far in the future. It probably means the user
        // entered some bogus number for the delay. Let's not panic.
        let millisecond_wait: i32 = match duration.as_millis().try_into() {
            Ok(value) => value,
            Err(_) => return Delay::Never,
        };

        // Ensure that we do not construct a NextEventDelay::Wait(0) result.
        match NonZeroI32::new(millisecond_wait) {
            Some(value) => Delay::Wait(value),
            None => Delay::Now,
        }
    }

    pub fn poll(&mut self) -> Vec<Instant> {
        self.schedule.sort_unstable();
        self.schedule.dedup();
        
        let mut ready_instants: Vec<Instant> = Vec::new();
        let mut remaining_instants: Vec<Instant> = Vec::new();
        let now = Instant::now();

        for instant in std::mem::take(&mut self.schedule) {
            if instant <= now {
                ready_instants.push(instant);
            } else {
                remaining_instants.push(instant);
            }
        }
        self.schedule = remaining_instants;

        return ready_instants;
    }
}
