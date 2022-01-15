// SPDX-License-Identifier: GPL-2.0-or-later

use crate::hook::Hook;
use crate::event::{Event, Channel};
use crate::capability::Capability;
use crate::key::Key;
use crate::state::State;
use crate::loopback::LoopbackHandle;

use std::collections::HashMap;

/// Represents a --withhold argument.
struct Withhold {
    /// TODO: doccomment.
    hooks: Vec<Hook>,
    /// Must only contain keys of type EV_KEY and must not specify any values.
    withholdable_keys: Vec<Key>,
}

impl Withhold {
    pub fn apply_to_all(&mut self, events: &[Event], events_out: &mut Vec<Event>, state: &mut State, loopback: &mut LoopbackHandle) {
        for event in events {
            apply(*event, events_out, &mut self.hooks, 0, state, loopback);
        }
    }

    pub fn apply_to_all_caps(&self, caps: &[Capability], caps_out: &mut Vec<Capability>) {
        // TODO: can the caps_out buffer be reused?
        let mut caps = caps.to_vec();
        let mut buffer: Vec<Capability> = Vec::new();
        for hook in &self.hooks {
            hook.apply_to_all_caps(&caps, &mut buffer);
            caps.clear();
            std::mem::swap(&mut caps, &mut buffer);
        }
        *caps_out = caps;
    }
}

fn apply(
        event: Event,
        events_out: &mut Vec<Event>,
        hooks: &mut [Hook],
        hook_index: usize,
        state: &mut State,
        loopback: &mut LoopbackHandle
) {
    // TODO: consider optimising stack usage.
    if let Some(hook) = hooks.get_mut(hook_index) {
        let mut buffer: Vec<Event> = Vec::new();
        hook.apply_to_all(&[event], &mut buffer, state, loopback);
        for next_event in buffer {
            apply(next_event, events_out, hooks, hook_index + 1, state, loopback);
        }
    } else {
        events_out.push(event);
    }
}

/// This part of the Withhold is actually responsible for blocking events until they are deemed
/// ready to release.
struct Blocker {
    keys: Vec<Key>,
    state: HashMap<Channel, ChannelState>,
}

enum ChannelState {
    Inactive,
    Withheld,
    Residual,
}

impl Blocker {
    fn apply(&mut self, event: Event, events_out: &mut Vec<Event>, hooks: &[Hook]) {
        unimplemented!()
    }
}
