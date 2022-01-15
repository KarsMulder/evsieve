// SPDX-License-Identifier: GPL-2.0-or-later

use crate::hook::Hook;
use crate::event::Event;
use crate::capability::Capability;
use crate::key::Key;
use crate::state::State;
use crate::loopback::LoopbackHandle;

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
            apply(&mut self.hooks, *event, events_out, state, loopback);
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
        hooks: &mut [Hook],
        event: Event,
        events_out: &mut Vec<Event>,
        state: &mut State,
        loopback: &mut LoopbackHandle
) {
    // TODO: consider optimising stack usage.
    if let [hook, remaining_hooks @ ..] = hooks {
        let mut buffer: Vec<Event> = Vec::new();
        hook.apply_to_all(&[event], &mut buffer, state, loopback);
        for next_event in buffer {
            apply(remaining_hooks, next_event, events_out, state, loopback);
        }
    } else {
        events_out.push(event);
    }
}
