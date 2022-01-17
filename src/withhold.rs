// SPDX-License-Identifier: GPL-2.0-or-later

use crate::event::{Event, Channel};
use crate::key::Key;
use crate::loopback::{LoopbackHandle, Token};
use crate::hook::{Trigger, TriggerResponse};

use std::collections::HashMap;

/// Used as array index to identify a Trigger in Withhold::triggers.
type TriggerIndex = usize;

/// Represents a --withhold argument.
pub struct Withhold {
    /// Copies of the triggers of the associated hooks.
    /// The index of each trigger in this vector must remain unchanged.
    triggers: Vec<Trigger>,

    /// Only withhold events that match one of the following keys.
    /// Regardless of what the keys say, the Withhold is only applicable to events of type EV_KEY.
    keys: Vec<Key>,

    channel_state: HashMap<Channel, ChannelState>,
}

impl Withhold {
    pub fn new(keys: Vec<Key>, triggers: Vec<Trigger>) -> Withhold {
        Withhold {
            keys, triggers,
            channel_state: HashMap::new(),
        }
    }

    pub fn apply_to_all(&mut self, events: &[Event], events_out: &mut Vec<Event>, loopback: &mut LoopbackHandle) {
        for event in events {
            self.apply(*event, events_out, loopback);
        }
    }

    fn apply(&mut self, event: Event, events_out: &mut Vec<Event>, loopback: &mut LoopbackHandle) {
        if ! event.ev_type().is_key() || ! self.keys.iter().any(|key| key.matches(&event)) {
            return events_out.push(event);
        }

        // Check with which indices this event is related in any way, as well as which triggers
        // just activated because of this event.
        let mut matchpos_trigger_indices: Vec<TriggerIndex> = Vec::new();
        let mut matchneg_trigger_indices: Vec<TriggerIndex> = Vec::new();
        let mut activated_trigger_indices: Vec<TriggerIndex> = Vec::new();
        for (index, trigger) in self.triggers.iter_mut().enumerate() {
            match trigger.apply(event, loopback) {
                TriggerResponse::None => (),
                TriggerResponse::MatchPositive => matchpos_trigger_indices.push(index),
                TriggerResponse::Activates => {
                    matchpos_trigger_indices.push(index);
                    activated_trigger_indices.push(index);
                },
                TriggerResponse::MatchNegative | TriggerResponse::Releases { .. }
                    => matchneg_trigger_indices.push(index),
            }
        }

        // If this event does not interact with any trigger, ignore it.
        if matchpos_trigger_indices.is_empty() && matchneg_trigger_indices.is_empty() {
            return events_out.push(event);
        }

        if event.value >= 1 {
            // If this event is a key_down event, associate all matching triggers with this channel.
            let state: &mut ChannelState = self.channel_state
                .entry(event.channel()).or_default();

            let (mut withholding_triggers, withheld_event) = match std::mem::take(state) {
                ChannelState::Withheld { withholding_triggers, withheld_event }
                    => (withholding_triggers, withheld_event),
                ChannelState::Inactive | ChannelState::Residual => (Vec::new(), event),
            };

            withholding_triggers.extend(matchpos_trigger_indices);
            withholding_triggers.sort_unstable();
            withholding_triggers.dedup();

            // TODO: Consider only withholding events with value 1.
            *state = ChannelState::Withheld { withholding_triggers, withheld_event };
        } else {
            // If it is a key_up event, all associated triggers are assumed to have released.
            let state = self.channel_state
                .remove(&event.channel())
                .unwrap_or(ChannelState::Inactive);

            match state {
                ChannelState::Withheld { withheld_event, .. } => {
                    events_out.push(withheld_event);
                    events_out.push(event);
                },
                ChannelState::Inactive => {
                    events_out.push(event);
                },
                ChannelState::Residual => {},
            }
        }

        // All events which were withheld by a trigger that just activated shall be considered
        // to have been consumed.
        for (_channel, state) in &mut self.channel_state {
            if let ChannelState::Withheld { withholding_triggers, .. } = state {
                for activated_index in &activated_trigger_indices {
                    if withholding_triggers.contains(activated_index) {
                        *state = ChannelState::Residual;
                        break;
                    }
                }
            }
        }
    }

    pub fn wakeup(&mut self, token: &Token, events_out: &mut Vec<Event>) {
        for trigger in &mut self.triggers {
            trigger.wakeup(token);
        }
        // TODO: quadratic algorithm?
        // At least don't run this loop for EVERY token.

        // Some trackers might have just expired. For all events that are being withheld,
        // check whether the respective triggers are still withholding them. Events that
        // are no longer withheld by any trigger shall be released bach to the stream.
        for (channel, state) in &mut self.channel_state {
            match state {
                ChannelState::Inactive | ChannelState::Residual => (),
                ChannelState::Withheld { withheld_event, ref mut withholding_triggers } => {
                    let triggers = &mut self.triggers;
                    withholding_triggers.retain(
                        |&index| triggers[index].has_active_tracker_matching_channel(*channel)
                    );
                    if withholding_triggers.is_empty() {
                        // TODO: consider preserving proper order.
                        events_out.push(*withheld_event);
                        *state = ChannelState::Inactive;
                    }
                }
            }
        }
    }
}

// TODO: Doccomment.
enum ChannelState {
    Withheld { withheld_event: Event, withholding_triggers: Vec<TriggerIndex> },
    Residual,
    Inactive,
}

impl Default for ChannelState {
    fn default() -> Self {
        ChannelState::Inactive
    }
}
