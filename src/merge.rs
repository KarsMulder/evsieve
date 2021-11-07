// SPDX-License-Identifier: GPL-2.0-or-later

use crate::key::Key;
use crate::event::{Event, Namespace};
use crate::domain::Domain;
use crate::capability::{Capability, CapMatch};
use crate::error::InternalError;
use crate::state::{State, MergeIndex};

/// Represents a --toggle argument.
pub struct Merge {
    keys: Vec<Key>,
    pub state_index: MergeIndex,
}

impl Merge {
    pub fn new(keys: Vec<Key>, state: &mut State) -> Merge {
        unimplemented!()
    }

    fn apply(&self, event: Event, output_events: &mut Vec<Event>, state: &mut State) {
        unimplemented!()
    }

    pub fn apply_to_all(&self, events: &[Event], output_events: &mut Vec<Event>, state: &mut State) {
        for &event in events {
            self.apply(event, output_events, state);
        }
    }

    pub fn apply_to_all_caps(&self, caps: &[Capability], output_caps: &mut Vec<Capability>) {
        unimplemented!()
    }
}
