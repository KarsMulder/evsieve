// SPDX-License-Identifier: GPL-2.0-or-later

use std::collections::HashMap;

use crate::capability::Capability;
use crate::event::{Channel, Event, EventType};
use crate::key::Key;
use crate::range::Range;

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

        match event.ev_type() {
            EventType::REL => {
                let residual = self.residuals.entry(event.channel()).or_insert(0.0);
                let desired_value = (event.value as f64) * self.factor + (*residual);
                let value_f64 = desired_value.floor();
        
                *residual = desired_value - value_f64;
                event.value = value_f64 as i32;
            },
            EventType::ABS => {
                event.value = map_abs_value(event.value, self.factor);
            },
            _ => {
                // The --scale argument is not meant to deal with events of types other than
                // rel and abs, but we might reach this point anyway due to having "" as key.
                // All events of type other than rel or abs shall be passed on verbatim.
            }
        }

        output_events.push(event);
    }

    /// The apply_ functions are analogous to the Map::apply_ equivalents.
    pub fn apply_to_all(&mut self, events: &[Event], output_events: &mut Vec<Event>) {
        for &event in events {
            self.apply(event, output_events);
        }
    }

    fn apply_to_cap(&self, cap: &Capability, output_caps: &mut Vec<Capability>) {
        match cap.code.ev_type() {
            EventType::ABS => {
                let bound_1 = cap.value_range.min.mul_f64_round(self.factor, round_abs_value);
                let bound_2 = cap.value_range.max.mul_f64_round(self.factor, round_abs_value);
                let range = Range::spanned_between(bound_1, bound_2);
                output_caps.push(cap.with_value(range));
            },
            EventType::REL => {
                let (max, min);
                if self.factor < 0.0 {
                    max = cap.value_range.min.mul_f64_round(self.factor, f64::ceil);
                    min = cap.value_range.max.mul_f64_round(self.factor, f64::floor);
                } else {
                    max = cap.value_range.max.mul_f64_round(self.factor, f64::ceil);
                    min = cap.value_range.min.mul_f64_round(self.factor, f64::floor);
                }
                let range = Range::spanned_between(max, min);
                output_caps.push(cap.with_value(range));
            },
            _ => output_caps.push(*cap),
        }
    }

    pub fn apply_to_all_caps(&self, caps: &[Capability], output_caps: &mut Vec<Capability>) {
        for cap in caps {
            self.apply_to_cap(cap, output_caps);
        }
    }
}

/// The rounding mode that is used for abs-type events.
fn round_abs_value(value: f64) -> f64 {
    value.round()
}
fn map_abs_value(value: i32, factor: f64) -> i32 {
    round_abs_value((value as f64) * factor) as i32
}
