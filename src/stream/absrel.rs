// SPDX-License-Identifier: GPL-2.0-or-later

use crate::{key::Key, event::Event, capability::Capability};

pub struct RelToAbs {
    keys: Vec<Key>,
}

impl RelToAbs {
    pub fn new(keys: Vec<Key>) -> RelToAbs {
        RelToAbs { keys }
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
