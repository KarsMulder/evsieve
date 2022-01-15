// SPDX-License-Identifier: GPL-2.0-or-later

use crate::hook::Hook;
use crate::event::{Event, Channel};
use crate::capability::Capability;
use crate::key::Key;
use crate::state::State;
use crate::loopback::LoopbackHandle;
use crate::hook::{Trigger, TriggerResponse};

use std::collections::HashMap;

/// Used as array index to identify a Trigger in Withhold::triggers.
type TriggerIndex = usize;

/// Represents a --withhold argument.
struct Withhold {
    /// Copies of the triggers of the associated hooks.
    /// The index of each trigger in this vector must remain unchanged.
    triggers: Vec<Trigger>,

    channel_state: HashMap<Channel, ChannelState>,
}

impl Withhold {
    pub fn apply_to_all(&mut self, events: &[Event], events_out: &mut Vec<Event>, loopback: &mut LoopbackHandle) {
        for event in events {
            self.apply(*event, events_out, loopback);
        }
    }

    pub fn apply(&mut self, event: Event, events_out: &mut Vec<Event>, loopback: &mut LoopbackHandle) {
        // Check with which indices this event is related in any way, as well as which triggers
        // just activated because of this event.
        let mut matching_trigger_indices: Vec<TriggerIndex> = Vec::new();
        let mut activated_trigger_indices: Vec<TriggerIndex> = Vec::new();
        for (index, trigger) in self.triggers.iter_mut().enumerate() {
            match trigger.apply(event, loopback) {
                TriggerResponse::None => (),
                TriggerResponse::Matched | TriggerResponse::Releases { .. }
                    => matching_trigger_indices.push(index),
                TriggerResponse::Activates => {
                    matching_trigger_indices.push(index);
                    activated_trigger_indices.push(index);
                }
            }
        }

        if matching_trigger_indices.is_empty() {
            events_out.push(event);
            return;
        }

        // Update which triggers withhold the events of this channel.
        let state: &mut ChannelState = self.channel_state
            .entry(event.channel()).or_default();
        let mut withholding_triggers = match std::mem::take(state) {
            ChannelState::Withheld { withholding_triggers } => withholding_triggers,
            ChannelState::Inactive | ChannelState::Residual => Vec::new(),
        };
        if event.value >= 1 {
            withholding_triggers.extend(matching_trigger_indices);
            withholding_triggers.sort_unstable();
            withholding_triggers.dedup();

            *state = ChannelState::Withheld { withholding_triggers };
        } else {
            withholding_triggers.retain(|index| !matching_trigger_indices.contains(index));
            *state = match withholding_triggers.is_empty() {
                true => ChannelState::Inactive,
                false => ChannelState::Withheld { withholding_triggers },
            }
            // TODO: Release withheld events.
        }

        // All events which were withheld by a trigger that just activated shall be considered
        // to have been consumed.
        for (_channel, state) in &mut self.channel_state {
            if let ChannelState::Withheld { withholding_triggers } = state {
                for activated_index in &activated_trigger_indices {
                    if withholding_triggers.contains(activated_index) {
                        *state = ChannelState::Residual;
                        break;
                    }
                }
            }
        }
        unimplemented!()
    }
}

// TODO: Doccomment.
enum ChannelState {
    Withheld { withholding_triggers: Vec<TriggerIndex> },
    Residual,
    Inactive,
}

impl Default for ChannelState {
    fn default() -> Self {
        ChannelState::Inactive
    }
}