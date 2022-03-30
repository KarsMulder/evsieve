// SPDX-License-Identifier: GPL-2.0-or-later

use crate::event::{Event, Channel, EventFlag};
use crate::key::Key;
use crate::loopback::{LoopbackHandle, Token};
use crate::stream::hook::{Trigger, TriggerResponse};

/// Represents a --withhold argument.
pub struct Withhold {
    /// Copies of the triggers of the associated hooks.
    triggers: Vec<Trigger>,

    /// Only withhold events that match one of the following keys.
    keys: Vec<Key>,

    channel_state: Vec<(Channel, ChannelState)>,
}

impl Withhold {
    pub fn new(keys: Vec<Key>, triggers: Vec<Trigger>) -> Withhold {
        Withhold {
            keys, triggers,
            channel_state: Vec::new(),
        }
    }

    pub fn apply_to_all(&mut self, events: &[Event], events_out: &mut Vec<Event>, loopback: &mut LoopbackHandle) {
        for event in events {
            self.apply(*event, events_out, loopback);
        }
    }

    fn apply(&mut self, mut event: Event, events_out: &mut Vec<Event>, loopback: &mut LoopbackHandle) {
        // Skip all events that did not match any preceding hook.
        if event.flags.get(EventFlag::Withholdable) {
            event.flags.unset(EventFlag::Withholdable);
        } else {
            return events_out.push(event);
        }

        // Check with which indices this event is related in any way, as well as which triggers
        // just activated because of this event.
        let mut activated_triggers: Vec<&Trigger> = Vec::new();
        for trigger in &mut self.triggers {
            match trigger.apply(event, loopback) {
                TriggerResponse::None
                | TriggerResponse::Matches
                | TriggerResponse::Releases => (),
                TriggerResponse::Activates => {
                    activated_triggers.push(trigger);
                },
            }
        }

        // If this is set to Some, then the provided event shall be added to events_out at the
        // end of the function, i.e. after all other withheld events have been released.
        //
        // Setting this to Some(event) is pretty much a delayed `events_out.push(event)` call.
        let final_event: Option<Event>;

        if self.keys.iter().any(|key| key.matches(&event)) {
            let current_channel_state: Option<&mut ChannelState> =
                self.channel_state.iter_mut()
                .find(|(channel, _state)| *channel == event.channel())
                .map(|(_channel, state)| state);

            if event.value == 1 {
                // Withhold the event. If there are no active trackers withholding this event,
                // it will be released later at `self.release_events()`.
                match current_channel_state {
                    None => self.channel_state.push(
                        (event.channel(), ChannelState::Withheld { withheld_event: event })
                    ),
                    Some(state @ &mut ChannelState::Residual) => {
                        *state =
                        
                        ChannelState::Withheld { withheld_event: event }
                    },
                    Some(ChannelState::Withheld { .. }) => {},
                }
                final_event = None;

            } else if event.value == 0 {
                // Remove a Residual block. If no Residual block is present, pass the event on.
                match current_channel_state {
                    // TODO: Consider whether a KEY_UP event should unconditionally release the withheld event.
                    None | Some(ChannelState::Withheld { .. }) => {
                        final_event = Some(event);
                    },
                    Some(ChannelState::Residual) => {
                        self.channel_state.retain(|(channel, _)| *channel != event.channel());
                        final_event = None;
                    }
                }
            } else {
                // KEY_REP events and other invalid values do not get withheld, but may be
                // passed on if the associated trackers are in invalid state.
                // 
                // TODO: The following code is what should happen, but the borrow checker doesn't
                // like it, so I'll have to think a bit about how to best refactor this.
                // 
                // TODO: write a unittest for this.

                // if ! self.triggers.iter().any(
                //    |trigger| trigger.has_active_tracker_matching_channel(event.channel())
                // ) {
                //     final_event = Some(event);
                // } else {
                //     final_event = None;
                // }

                final_event = None;
            }
        } else {
            // This event can not be withheld. Add it to the stream after releasing past events.
            final_event = Some(event);
        }

        // All events which were withheld by a trigger that just activated shall be considered
        // to have been consumed and their states are to be set to Residual.
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

        // All events which are no longer withheld by any trigger shall be released.
        self.release_events(events_out);

        if let Some(event) = final_event {
            events_out.push(event);
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
        self.release_events(events_out);
    }

    /// Writes all events that are not withheld by any trigger to the output stream.
    fn release_events(&mut self, events_out: &mut Vec<Event>) {
        let triggers = &self.triggers;
        self.channel_state.retain(|(channel, state)| {
            if let ChannelState::Withheld { withheld_event } = state {
                let is_still_withheld = triggers.iter().any(|trigger|
                    trigger.has_active_tracker_matching_channel(*channel)
                );
                if ! is_still_withheld {
                    events_out.push(*withheld_event);
                    return false;
                }
            }
            true
        });
    }
}

/// For each channel, at most one event can be withheld. This withheld event is always a
/// KEY_DOWN event. Subsequent KEY_DOWN events that arrive while an event is being withheld
/// shall be dropped. The event is withheld as long as some tracker returns true for
/// `has_active_tracker_matching_channel(event.channel())`.
/// 
/// If a trigger activates and said trigger has a tracker matching the event's channel, the
/// state of that channel shall become Residual instead. When a channel is in residual state,
/// the next KEY_UP event matching that channel gets dropped. After dropping a KEY_UP event,
/// the state of the corresponding channel returns to undefined. Furthermore, a KEY_DOWN event
/// arriving to a channel in Residual state cancels the Residual state and sets it back to
/// Withheld.
#[derive(Debug, Clone, Copy)]
enum ChannelState {
    Withheld { withheld_event: Event },
    Residual,
}
