// SPDX-License-Identifier: GPL-2.0-or-later

use crate::event::{Event, Channel};
use crate::key::Key;
use crate::loopback::{LoopbackHandle, Token};
use crate::hook::{Trigger, TriggerResponse};

use std::collections::HashMap;

/// Represents a --withhold argument.
pub struct Withhold {
    /// Copies of the triggers of the associated hooks.
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
        if ! event.ev_type().is_key() {
            return events_out.push(event);
        }

        // Check with which indices this event is related in any way, as well as which triggers
        // just activated because of this event.
        let mut activated_triggers: Vec<&Trigger> = Vec::new();
        let mut any_trigger_matched: bool = false;
        for trigger in &mut self.triggers {
            match trigger.apply(event, loopback) {
                TriggerResponse::None => (),
                TriggerResponse::Activates => {
                    activated_triggers.push(trigger);
                    any_trigger_matched = true;
                },
                TriggerResponse::Matches | TriggerResponse::Releases
                    => any_trigger_matched = true,
            }
        }

        // If this event does not interact with any trigger, ignore it.
        if ! any_trigger_matched {
            return events_out.push(event);
        }

        if self.keys.iter().any(|key| key.matches(&event)) {
            // If this event matched a trigger and matches a key of self, then withhold it if
            // it is an activating event (value >= 1) or release it if it is a releasing event.
            if event.value >= 1 {
                // If this event is a key_down event, associate all matching triggers with
                // this channel.
                let state: &mut ChannelState = self.channel_state
                    .entry(event.channel()).or_default();

                match state {
                    ChannelState::Withheld { .. } => {},
                    ChannelState::Inactive => {
                        *state = ChannelState::Withheld {
                            // TODO: consider only storing KEY_DOWN events and not KEY_HOLD.
                            withheld_event: event,
                        };
                    }
                    ChannelState::Residual => {},
                };
            } else {
                // If it is a key_up event, all associated triggers are assumed to have been
                // released. To make this assumption true, the associated --hook's must only
                // use EV_KEY-type keys with default values.
                let state = self.channel_state
                    .remove(&event.channel())
                    .unwrap_or(ChannelState::Inactive);

                match state {
                    ChannelState::Withheld { withheld_event } => {
                        events_out.push(withheld_event);
                        events_out.push(event);
                    },
                    ChannelState::Inactive => {
                        events_out.push(event);
                    },
                    ChannelState::Residual => {},
                }
            }
        } else {
            events_out.push(event);
        }

        // All events which were withheld by a trigger that just activated shall be considered
        // to have been consumed.
        for (channel, state) in &mut self.channel_state {
            if let ChannelState::Withheld { .. } = state {
                for trigger in &activated_triggers {
                    if trigger.has_tracker_matching_channel(*channel) {
                        *state = ChannelState::Residual;
                        break;
                    }
                }
            }
        }
    }

    pub fn wakeup(&mut self, token: &Token, events_out: &mut Vec<Event>) {
        let mut some_tracker_expired = false;
        for trigger in &mut self.triggers {
            if trigger.wakeup(token) {
                some_tracker_expired = true;
            }
        }
        if ! some_tracker_expired {
            return;
        }

        // Some trackers have expired. For all events that are being withheld, check
        // whether the respective triggers are still withholding them. Events that
        // are no longer withheld by any trigger shall be released bach to the stream.
        for (channel, state) in &mut self.channel_state {
            match state {
                ChannelState::Inactive | ChannelState::Residual => (),
                ChannelState::Withheld { withheld_event } => {
                    let should_still_withhold = self.triggers.iter().any(
                        |trigger| trigger.has_active_tracker_matching_channel(*channel)
                    );
                    if ! should_still_withhold {
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
#[derive(Debug)]
enum ChannelState {
    Withheld { withheld_event: Event },
    Residual,
    Inactive,
}

impl Default for ChannelState {
    fn default() -> Self {
        ChannelState::Inactive
    }
}
