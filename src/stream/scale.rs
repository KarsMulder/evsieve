// SPDX-License-Identifier: GPL-2.0-or-later

use std::collections::HashMap;

use crate::capability::Capability;
use crate::event::{Channel, Event, EventType};
use crate::key::Key;
use crate::range::Interval;

pub struct Scale {
    input_keys: Vec<Key>,
    factor: f64,

    /// A map that contains for each map how much value should've been sent over this channel, but hasn't
    /// because we can only sent integer values. For example, if rel:x:4 gets processed by a factor=0.4
    /// map, then we want to send rel:x:1.6, but we can only send integer values, so instead we send
    /// rel:x:1 and add 0.6 to the residual. The residual will be added to the value of the same event on
    /// the same channel.
    /// 
    /// The residuals only apply to rel-type events, because it doesn't make sense to apply them to abs-type
    /// events: if abs:x:1 gets sent multiple times, then we clearly want each of them to map to the same
    /// value each time.
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
        let output_cap = cap.map_values(|set| set.map(|interval| {
            match cap.code.ev_type() {
                EventType::ABS => {
                    let bound_1 = mul_f64_round(interval.min, self.factor, round_abs_value);
                    let bound_2 = mul_f64_round(interval.max, self.factor, round_abs_value);
                    let interval_out = Interval::spanned_between(bound_1, bound_2);
                    Some(interval_out)
                },
                EventType::REL => {
                    // Depending on the value of the residual, (factor*value) can always be rounded
                    // either up or downwards. This means that the upper bound of the range must be
                    // rounded up, and the lower bound must be rounded down.
                    let (max, min);
                    if self.factor < 0.0 {
                        max = mul_f64_round(interval.min, self.factor, f64::ceil);
                        min = mul_f64_round(interval.max, self.factor, f64::floor);
                    } else {
                        max = mul_f64_round(interval.max, self.factor, f64::ceil);
                        min = mul_f64_round(interval.min, self.factor, f64::floor);
                    }
                    let interval_out = Interval::spanned_between(max, min);
                    Some(interval_out)
                },
                _ => Some(interval),
            }
        }));

        output_caps.push(output_cap);
    }

    pub fn apply_to_all_caps(&self, caps: &[Capability], output_caps: &mut Vec<Capability>) {
        for cap in caps {
            self.apply_to_cap(cap, output_caps);
        }
    }
}

fn mul_f64_round(value: i32, factor: f64, rounding_mode: impl Fn(f64) -> f64) -> i32 {
    rounding_mode(value as f64 * factor) as i32
}

/// The rounding mode that is used for abs-type events.
fn round_abs_value(value: f64) -> f64 {
    // A simple value.round() is unacceptable because it rounds away from zero, which could cause an unnatural
    // move in the axis for example in --scale factor=0.5:
    //
    // In:  -5  -4  -3  -2  -1  0  1  2  3  4  5
    // Out: -3  -2  -2  -1  -1  0  1  1  2  2  3
    //
    // Notice how the number 0 shows up a single time, whereas the other numbers show up twice.
    // There are two common types of ranges for axes encounted in the wild: [0, (2^n)-1] and [-x, x]
    // Ideally, we would use a rounding mode that accomodates both.
    //
    // In the latter case would be ideally served if the range of what maps to 0 is as big as possible.
    // That would be trunc(), but trunc() is just as ridiculous as round(), just in the different direction.
    //
    // The former case, it is fine for 0 to be an edge case that is only met if the input stick touches
    // the absolute edge of the original range. It would also be nice that if a range of [0, 127] after
    // being halved became [0, 63] while [0, 128] becomes [0, 64] after being halved.
    //
    // With the above considerations in mind, I have decided the ideal rounding algorithm to be:
    // Rounds as usual for everyithing that is not 0.5 away from an integer. If it is exactly 0.5 away
    // from an integer, rounds down.


    // Using == for comparison with float looks strange, but is absolutely correct here. There is only
    // exactly one floating point value that the fractional could take that makes round() return an
    // undesirable value. If fract() were 0.500000000001 or 0.4999999999, then round() would do what
    // it is supposed to do.
    //
    // Also, negative numbers have negative `fract()`s, so this implicitly checks that `value > 0`. For
    // negative values, .round() already rounds down anyway.
    let mut res = value.round();
    if value.fract() == 0.5 {
        res -= 1.0;
    }
    res
}

fn map_abs_value(value: i32, factor: f64) -> i32 {
    round_abs_value((value as f64) * factor) as i32
}
