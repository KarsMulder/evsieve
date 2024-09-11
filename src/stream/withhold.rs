// SPDX-License-Identifier: GPL-2.0-or-later

use crate::capability::Capability;
use crate::event::{Event, Channel};
use crate::key::Key;
use crate::loopback::{LoopbackHandle, Token};
use crate::state::State;
use crate::stream::hook::{Trigger, TriggerResponse};

use super::hook::Hook;

/// Represents a --withhold argument.
pub struct Withhold {
    /// Only withhold events that match one of the following keys.
    keys: Vec<Key>,

    /// For each WithholdChannel, at most one event can be withheld simultaneously. In `channel_state`,
    /// we keep track for each WithholdChannel which event is being withheld there (if any). It can also
    /// contain instructions like "last KEY_DOWN event on this channel was dropped, so drop the next
    /// KEY_UP event".
    channel_state: Vec<(WithholdChannel, ChannelState)>,
}

/// Represents a group of one or more --hook arguments followed up by a single --withhold argument.
pub struct HookGroup {
    hooks: Vec<Hook>,
    withhold: Withhold,
}

impl HookGroup {
    pub fn new(hooks: Vec<Hook>, withhold: Withhold) -> HookGroup {
        HookGroup {
            hooks,
            withhold,
        }
    }
}

impl HookGroup {
    pub fn apply_to_all(&mut self, events_in: &[Event], events_out: &mut Vec<Event>, state: &mut State, loopback: &mut LoopbackHandle) {
        // This function is basically a mini-stream in the bigger `Stream` class. This mini-stream tracks
        // not only events, but also tracks additional information for each event. Specifically, for each event,
        // we want to keep track of how each hook reacted to said event.
        let mut events: Vec<(Event, TriggerResponseRecord)> = events_in.iter().map(|&event|
            (event, TriggerResponseRecord::new())
        ).collect();

        let mut buffer: Vec<(Event, TriggerResponseRecord)> = Vec::new();

        // Pass all events to the hooks, one hook at a time. Keep a record of how each event reacted with
        // each trigger.
        //
        // It is important that the outer loop loops over hooks and the inner hook loops over events to ensure
        // that a HookGroup functions identically to a series of Hooks within a Stream.
        for (hook_idx, hook) in self.hooks.iter_mut().enumerate() {
            let hook_idx = HookIdx(hook_idx);

            for (event, response_record) in events.drain(..) {
                let response = hook.trigger.apply(event, loopback);
                let record_for_current_event = response_record.with_response(&hook.trigger, hook_idx, event, response);
                hook.actuator.apply_response(response, event, record_for_current_event, &mut buffer, state);
            }

            std::mem::swap(&mut events, &mut buffer);
            // `buffer.clear()` is unnecessary here because `events.drain(..)` should've emptied the event vector.
            assert!(buffer.is_empty());
        }

        // TODO: unnecessay allocation
        let triggers: Box<[&Trigger]> = self.hooks.iter().map(|hook| &hook.trigger).collect();
        for (event, response_record) in events {
            self.withhold.apply(event, response_record, events_out, &triggers);
        }
    }

    pub fn apply_to_all_caps(&self, caps_in: &[Capability], caps_out: &mut Vec<Capability>) {
        let mut caps: Vec<Capability> = caps_in.to_vec();
        let mut buffer: Vec<Capability> = Vec::new();
        for hook in &self.hooks {
            hook.apply_to_all_caps(&caps, &mut buffer);
            std::mem::swap(&mut caps, &mut buffer);
            buffer.clear();
        }
        self.withhold.apply_to_all_caps(&caps, caps_out);
    }

    pub fn wakeup(&mut self, token: &Token, events_out: &mut Vec<Event>) {
        let mut some_tracker_expired = false;
        let triggers = self.hooks.iter_mut().map(|hook| &mut hook.trigger);
        for trigger in triggers {
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
        let triggers: Vec<&Trigger> = self.hooks.iter_mut().map(|hook| &hook.trigger).collect();
        self.withhold.release_events(&triggers, events_out);
    }
}

/// Represents an index into the vector `HookGroup::hooks`.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct HookIdx(usize);

/// At most one event per WithholdChannel can be withheld at the same time.
/// 
/// Most of the program works based on just (event_channel) channels, but that can lead to
/// some really confusing situations, such as:
/// 
///     --input $DEVICE
///     --hook key:a key:z
///     --hook key:b send-key=key:a
///     --hook key:a key:x
///     --withhold
/// 
/// Now imagine the following sequence of key presses: B down, A down, Z down, AZB up.
/// At this point, a key:a event has been withheld by both the first and third hook.
/// The first key:a event from the first hook needs to be dropped, but the key:a event
/// from the third hook needs to be released.
/// 
/// To avoid such situations, we separate events not just on channel, but also based on
/// the first hook that the event has seen. This this case, the events from the keyboard
/// would correspond to the (key:a, 0) WithholdChannel, but events generated by the second hook
/// would correspond to the (key:a, 2) WithholdChannel. By not letting events of different
/// WithholdChannels interfere with each other, many situations get simplified.
#[derive(Clone, Copy, PartialEq, Eq)]
struct WithholdChannel {
    event_channel: Channel,
    first_hook: HookIdx,
}

impl WithholdChannel {
    /// Returns `None` if this event has not passed any hooks (e.g. events that were generated by the
    /// last hook). Events that have not passed any hooks should of course not be withheld.
    fn from_event_and_response_record(event: Event, response_record: &TriggerResponseRecord) -> Option<WithholdChannel> {
        Some(WithholdChannel {
            event_channel: event.channel(),
            first_hook: response_record.trigger_status.first()?.0,
        })
    }

    /// Tells you whether the state of a certain hook should affect whether events are being withheld
    /// under this channel. For example, consider the following arguments:
    /// 
    ///     --input $DEVICE
    ///     --hook key:a key:z
    ///     --hook key:b send-key=key:a
    ///     --hook key:a key:x
    ///     --withhold
    /// 
    /// If a key:a event was generated by the second hook, then whatever the first hook does should have
    /// no influence in whether that event gets withheld or not. For such events, `is_affected_by_hook`
    /// will return true for `HookIdx(2)``, but false for `HookIdx(0)` and `HookIdx(1)`.
    fn is_affected_by_hook(&self, hook_idx: HookIdx) -> bool {
        hook_idx >= self.first_hook
    }
}

/// Represents which role a Trigger plays in a certain channel being withheld.
#[derive(Clone, Copy, PartialEq, Eq)]
enum TriggerStatus {
    /// This trigger is currently active on this channel. The event must be withheld until all active
    /// triggers turn inactive.
    Active,
    /// This trigger is currently inactive on this channel and therefore does not block events on this channel.
    Inactive,
}

#[derive(Clone)]
struct TriggerResponseRecord {
    trigger_status: Vec<(HookIdx, TriggerStatus)>,
    activated_triggers: Vec<HookIdx>,
    any_trigger_interacts: bool,
}

impl TriggerResponseRecord {
    fn new() -> Self {
        Self {
            trigger_status: Vec::new(),
            activated_triggers: Vec::new(),
            any_trigger_interacts: false,
        }
    }

    fn with_response(mut self, trigger: &Trigger, hook_idx: HookIdx, event: Event, response: TriggerResponse) -> TriggerResponseRecord {
        match response {
            TriggerResponse::None => {},
            TriggerResponse::Interacts
            | TriggerResponse::Releases => {
                self.any_trigger_interacts = true;
            },
            TriggerResponse::Activates => {
                self.activated_triggers.push(hook_idx);
                self.any_trigger_interacts = true;
            },
        }
        // TODO: MEDIUM-PRIORITY maybe this information should be returned by trigger.apply()?
        let trigger_status = match trigger.has_active_tracker_matching_channel(event.channel()) {
            true => TriggerStatus::Active,
            false => TriggerStatus::Inactive,
        };

        self.trigger_status.push((hook_idx, trigger_status));
        
        self
    }
}

impl Default for TriggerResponseRecord {
    fn default() -> Self {
        Self::new()
    }
}

impl Withhold {
    pub fn new(keys: Vec<Key>) -> Withhold {
        Withhold {
            keys,
            channel_state: Vec::new(),
        }
    }

    fn apply(&mut self, event: Event, response_record: TriggerResponseRecord, events_out: &mut Vec<Event>, triggers: &[&Trigger]) {
        // Skip all events that did not match any preceding hook.
        if ! response_record.any_trigger_interacts {
            return events_out.push(event);
        }

        let withhold_channel = match WithholdChannel::from_event_and_response_record(event, &response_record) {
            Some(channel) => channel,
            // If `from_event_and_response_record` returns None, then this event didn't go past any hooks,
            // and therefore should not be withheld.
            None => return events_out.push(event),
        };

        // If this is set to Some, then the provided event shall be added to events_out at the
        // end of the function, i.e. after all other withheld events have been released.
        //
        // Setting this to Some(event) is pretty much a delayed `events_out.push(event)` call.
        let final_event: Option<Event>;

        if self.keys.iter().any(|key| key.matches(&event)) {
            // Decide whether or not to hold this event.

            let current_channel_state: Option<&mut ChannelState> =
                self.channel_state.iter_mut()
                .find(|(channel, _state)| *channel == withhold_channel)
                .map(|(_channel, state)| state);

            let any_tracker_active_on_channel = response_record.trigger_status.iter()
                .any(|(_idx, status)| *status == TriggerStatus::Active);

            if any_tracker_active_on_channel {
                // If the event value were zero, then the constraint of "no custom value declarations"
                // for the preceding hooks should have made sure that no trackers are active on this
                // channel because a value-zero event would deactivate all of them.
                debug_assert!(event.value != 0);

                if event.value == 1 {
                    // Withhold the event unless an event was already being withheld.
                    match current_channel_state {
                        None => self.channel_state.push(
                            (withhold_channel, ChannelState::Withheld { withheld_event: event })
                        ),
                        Some(state @ &mut ChannelState::Residual) => {
                            *state = ChannelState::Withheld { withheld_event: event }
                        },
                        Some(ChannelState::Withheld { .. }) => {},
                    }
                    final_event = None;
                } else {
                    // Drop all repeat events on channels that have an active tracker.
                    final_event = None;
                }
            } else { // No trackers active at the event's channel.
                if event.value == 0 {
                    // Due to the restrictions on the hooks (i.e. only default values), an event of
                    // value zero cannot possibly contribute to activating any hook, so we are free
                    // to pass on this event unless a residual state instructs us to drop this event.

                    match current_channel_state {
                        None | Some(ChannelState::Withheld { .. }) => {
                            // The withheld event will be released by a later piece of code.
                            final_event = Some(event);
                        },
                        Some(ChannelState::Residual) => {
                            // Drop this event and clear the residual state.
                            self.channel_state.retain(|(channel, _)| *channel != withhold_channel);
                            final_event = None;
                        }
                    }
                } else {
                    // In this case, all corresponding trackers are probably in invalid state.
                    // Anyway, knowing that no trackers are active means that this event won't
                    // contribute to activating a hook, and its value being nonzero means that
                    // we don't have to deal with the residual rules, so we can pass this event
                    // on.
                    final_event = Some(event);
                }
            }
        } else {
            // This event can not be withheld. Add it to the stream after releasing past events.
            final_event = Some(event);
        }

        // All events which were withheld by a trigger that just activated shall be considered
        // to have been consumed and their states are to be set to Residual.
        for (channel, state) in &mut self.channel_state {
            if let ChannelState::Withheld { .. } = state {
                for &hook_idx in &response_record.activated_triggers {
                    if channel.is_affected_by_hook(hook_idx) {
                        let trigger = &triggers[hook_idx.0];
                        if trigger.has_tracker_matching_channel(channel.event_channel) {
                            *state = ChannelState::Residual;
                            break;
                        }
                    }
                }
            }
        }

        // All events which are no longer withheld by any trigger shall be released.
        self.release_events(triggers, events_out);

        if let Some(event) = final_event {
            events_out.push(event);
        }
    }

    /// Writes all events that are not withheld by any trigger to the output stream.
    fn release_events(&mut self, triggers: &[&Trigger], events_out: &mut Vec<Event>) {
        self.channel_state.retain(|(channel, state)| {
            if let ChannelState::Withheld { withheld_event } = state {
                let mut related_triggers = triggers.iter().skip(channel.first_hook.0);
                let is_still_withheld = related_triggers.any(|trigger|
                    trigger.has_active_tracker_matching_channel(channel.event_channel)
                );
                if ! is_still_withheld {
                    events_out.push(*withheld_event);
                    return false;
                }
            }
            true
        });
    }

    fn apply_to_all_caps(&self, caps: &[Capability], caps_out: &mut Vec<Capability>) {
        caps_out.extend_from_slice(&caps);
    }
}

/// For each `WithholdChannel`, at most one event can be withheld. This withheld event is always
/// a KEY_DOWN event. Subsequent KEY_DOWN events that arrive while an event is being withheld
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
