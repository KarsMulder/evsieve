// SPDX-License-Identifier: GPL-2.0-or-later

use std::collections::HashMap;

use crate::capability::Capability;
use crate::event::{Channel, Event};
use crate::key::Key;

pub struct Scale {
    input_keys: Vec<Key>,
    factor: f64,

    /// A map that contains for each map how much value should've been sent over this channel, but hasn't
    /// because we can only sent integer values. For example, if rel:x:4 gets processed by a factor=0.4
    /// map, then we want to send rel:x:1.6, but we can only send integer values, so instead we send
    /// rel:x:1 and add 0.6 to the residual. The residual will be added to the value of the same event on
    /// the same channel.
    residuals: HashMap<Channel, f64>,
}

impl Scale {
    pub fn new(input_keys: Vec<Key>, factor: f64) -> Self {
        Self {
            input_keys,
            factor,
            residuals: HashMap::new(),
        }
    }

    fn apply(&mut self, mut event: Event, output_events: &mut Vec<Event>) {
        if ! self.input_keys.iter().any(|key| key.matches(&event)) {
            return output_events.push(event);
        }

        // TODO (High Priority): The following approach is only sensible for EV_REL-type events.
        // Figure out what we want to do for EV_ABS-type events.
        let residual = self.residuals.entry(event.channel()).or_insert(0.0);
        let desired_value = (event.value as f64) * self.factor + (*residual);
        let value_f64 = desired_value.floor();

        event.value = value_f64 as i32;
        *residual = desired_value - value_f64;
        output_events.push(event);
    }

    /// The apply_ functions are analogous to the Map::apply_ equivalents.
    pub fn apply_to_all(&mut self, events: &[Event], output_events: &mut Vec<Event>) {
        for &event in events {
            self.apply(event, output_events);
        }
    }

    pub fn apply_to_all_caps(&self, caps: &[Capability], output_caps: &mut Vec<Capability>) {
        output_caps.extend(caps);
    }
}