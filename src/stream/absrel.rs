// SPDX-License-Identifier: GPL-2.0-or-later

use std::collections::HashMap;

use crate::{key::Key, event::{Event, Channel}, capability::Capability, range::Range};

pub struct RelToAbs {
    input_key: Key,
    output_key: Key,
    output_range: Range,
    speed: f64,
    
    // For each channel that this argument may output, keeps track of the current value it has.
    state: HashMap<Channel, i32>,
}

impl RelToAbs {
    pub fn new(input_key: Key, output_key: Key, output_range: Range, speed: f64) -> RelToAbs {
        RelToAbs {
            input_key, output_key, output_range, speed,
            state: HashMap::new(),
        }
    }

    /// Analogue of Map::apply_to_all().
    pub fn apply_to_all(&self, events: &[Event], output_events: &mut Vec<Event>) {
        unimplemented!()
    }

    /// Analogue of Map::apply_to_all_caps().
    pub fn apply_to_all_caps(&self, caps: &[Capability], output_caps: &mut Vec<Capability>) {
        unimplemented!()
    }
}
