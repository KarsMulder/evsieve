// SPDX-License-Identifier: GPL-2.0-or-later

use crate::error::Context;
use crate::range::{Interval, Set};
use crate::key::Key;
use crate::event::{Event, Channel};
use crate::state::State;
use crate::subprocess;
use crate::loopback;
use crate::loopback::LoopbackHandle;
use crate::capability::{Capability, Certainty};
use crate::time::Duration;
use std::collections::HashSet;

use super::sink::Sink;

// TODO: HIGH-PRIORITY Check whether the ordering behaviour of --withhold is consistent
// with --hook send-key.

pub type Effect = Box<dyn Fn(&mut State)>;

/// Represents the point at time after which a pressed tracker is no longer valid.
/// Usually determined by the --hook period= clause.
pub enum ExpirationTime {
    Never,
    Until(loopback::Token),
}

enum TrackerState {
    /// This tracker's corresponding key is held down. Also keeps track of how much time is
    /// left until this tracker expires due to a period= clause. If no period= clause was
    /// specified, then its expiration time shall be ExpirationTime::Never.
    Active(ExpirationTime),
    /// This tracker's corresponding key is not held down.
    Inactive,
    /// Based on the events that were received by this Tracker, the state should be active,
    /// but it is counted as inactive due to some circumstances, e.g. because the period
    /// in which the hook must be triggered expired, or because this tracker was activated
    /// before its predecessors in a sequential hook.
    /// 
    /// To activate this tracker, it first needs to return to Inactive and then activate.
    Invalid,
}

impl TrackerState {
    fn is_active(&self) -> bool {
        match self {
            TrackerState::Active (_) => true,
            TrackerState::Inactive | TrackerState::Invalid => false,
        }
    }
}

/// A tracker is used to track whether a certain key is held down. This is useful for --hook type
/// arguments.
struct Tracker {
    key: Key,
    range: Interval,

    /// The state is mutable at runtime. It reflects whether the key tracked by this tracker
    /// is currently pressed or not, as well as which event triggered it and when.
    state: TrackerState,
}

impl Tracker {
    fn new(mut key: Key) -> Tracker {
        let range = key.pop_value().unwrap_or_else(|| Interval::new(Some(1), None));
        Tracker {
            key,
            range,
            state: TrackerState::Inactive,
        }
    }

    /// Returns true if this event might interact with this tracker in some way.
    fn matches(&self, event: &Event) -> bool {
        self.key.matches(event)
    }

    /// Returns true if any event with the given channel might interact with this
    /// tracker in some way.
    fn matches_channel(&self, channel: Channel) -> bool {
        self.key.matches_channel(channel)
    }

    /// Returns whether this event would turn this tracker on or off.
    /// Only returns sensible values if self.matches(event) is true.
    fn activates_by(&self, event: Event) -> bool {
        self.range.contains(event.value)
    }

    fn is_active(&self) -> bool {
        match self.state {
            TrackerState::Active(_) => true,
            TrackerState::Invalid | TrackerState::Inactive => false,
        }
    }

    /// Like Clone::clone, but does not clone the runtime state of the Tracker.
    fn clone_empty(&self) -> Tracker {
        Tracker {
            key: self.key.clone(),
            range: self.range,
            state: TrackerState::Inactive,
        }
    }
}

/// The Trigger is the inner part of the hook that keeps track of when the hook is supposed to
/// activate.
pub struct Trigger {
    /// If Some, then all trackers must be activated within a certain duration from the first
    /// tracker to activate in order to trigger the hook.
    period: Option<Duration>,
    /// If true, then all trackers belonging to this Trigger must be triggered in sequential
    /// order. If a tracker is activated while its previous tracker is still inactive, then
    /// that tracker becomes invalid.
    sequential: bool,
    breaks_on: Vec<Key>,

    trackers: Vec<Tracker>,
    state: TriggerState,
}

/// Returned by Trigger::apply to inform the caller what effect the provided event had on
/// the hook.
#[derive(Clone, Copy)]
pub enum TriggerResponse {
    /// This event does not interact with this hook in any way.
    None,
    /// This event may have changed the state of this trigger some way. Does not guarantee that
    /// this event actually matches one of its keys or that the state was actually changed.
    /// Guarantees that the trigger was not activated or released.
    Interacts,
    /// The hook has activated because of this event. Its effects should be triggered.
    Activates,
    /// The hook has released because of this event. Its on-release effects should be triggered.
    Releases,
}

#[derive(Clone, Copy)]
enum TriggerState {
    /// All trackers are currently pressed.
    Active,
    /// Not all trackers are currently pressed.
    Inactive,
}

impl Trigger {
    pub fn new(keys: Vec<Key>, breaks_on : Vec<Key>, period: Option<Duration>, sequential: bool) -> Trigger {
        let trackers = keys.into_iter().map(Tracker::new).collect();
        Trigger {
            period, trackers, sequential, breaks_on,
            state: TriggerState::Inactive,
        }
    }

    pub fn apply(&mut self, event: Event, loopback: &mut LoopbackHandle) -> TriggerResponse {
        let mut any_tracker_matched: bool = false;

        for tracker in self.trackers.iter_mut()
            .filter(|tracker| tracker.matches(&event))
        {
            any_tracker_matched = true;

            if tracker.activates_by(event) {
                match tracker.state {
                    // If this tracker was inactive, activate it.
                    TrackerState::Inactive => {
                        // Note: if this hook is sequential, this activation may get invalidated
                        // later in this function.
                        tracker.state = TrackerState::Active(
                            acquire_expiration_token(self.period, loopback)
                        );
                    },
                    TrackerState::Active(..) | TrackerState::Invalid => {},
                }
            } else {
                tracker.state = TrackerState::Inactive;
            };
        }
        
        if ! any_tracker_matched {
            // If none of the trackers match this event, but it does match one of the breaks-on
            // notes, then invalidate all trackers.
            if self.breaks_on.iter().any(|key| key.matches(&event)) {
                let mut any_tracker_invalidated = false;

                for tracker in &mut self.trackers {
                    match tracker.state {
                        TrackerState::Active(_) => {
                            tracker.state = TrackerState::Invalid;
                            any_tracker_invalidated = true;
                            // TODO: LOW-PRIORITY Cancel token.
                        },
                        TrackerState::Inactive | TrackerState::Invalid => {},
                    }
                }

                if ! any_tracker_invalidated {
                    return TriggerResponse::None;
                }
            } else {
                // No trackers care about this event.
                return TriggerResponse::None;
            }
        }

        if self.sequential {
            // Invalidate all trackers that activated out of order.
            self.trackers.iter_mut()
                // Skip all trackers that are consecutively active from the start.
                .skip_while(|tracker| tracker.is_active())
                // ... then find all trackers that are active but not consecutively so.
                .filter(|tracker| tracker.is_active())
                // ... and invalidate them.
                // TODO: LOW-PRIORITY Consider canceling the activation token.
                .for_each(|tracker| tracker.state = TrackerState::Invalid);
        }

        // Check if we transitioned between active and inactive.
        let all_trackers_active = self.trackers.iter().all(|tracker| tracker.state.is_active());

        match (self.state, all_trackers_active) {
            (TriggerState::Inactive, true) => {
                self.state = TriggerState::Active;
                // TODO: LOW-PRIORITY Cancel tokens?
                for tracker in &mut self.trackers {
                    tracker.state = TrackerState::Active(ExpirationTime::Never);
                }
                TriggerResponse::Activates
            },
            (TriggerState::Active, false) => {
                self.state = TriggerState::Inactive;
                TriggerResponse::Releases
            },
            (TriggerState::Active {..}, true) | (TriggerState::Inactive, false)
                => TriggerResponse::Interacts,
        }
    }

    /// Release a tracker that has expired. If a tracker expired, returns the associated key.
    /// It is important that the Tokens are unique for this function to work correctly.
    /// 
    /// Returns true if at least one tracker expired. Returns false otherwise.
    pub fn wakeup(&mut self, token: &loopback::Token) -> bool {
        let mut result = false;
        for tracker in &mut self.trackers {
            match tracker.state {
                TrackerState::Inactive => {},
                TrackerState::Invalid => {},
                TrackerState::Active(ExpirationTime::Never) => {},
                TrackerState::Active(ExpirationTime::Until(ref other_token)) => {
                    if token == other_token {
                        // This tracker expired.
                        tracker.state = TrackerState::Invalid;
                        result = true;
                    }
                }
            }
        }
        result
    }

    /// Returns true if any of the active trackers might have been activated by an event
    /// with the provided channel, regardless of whether that channel actually activated them.
    pub fn has_active_tracker_matching_channel(&self, channel: Channel) -> bool {
        self.trackers.iter()
            .filter(|tracker| tracker.is_active())
            .any(   |tracker| tracker.matches_channel(channel))
    }

    /// Returns true if any of the might be activated by an event with the provided channel.
    pub fn has_tracker_matching_channel(&self, channel: Channel) -> bool {
        self.trackers.iter()
            .any(|tracker| tracker.matches_channel(channel))
    }

    /// Like Clone::clone, but does not clone the runtime state of the Trigger.
    pub fn clone_empty(&self) -> Trigger {
        Trigger {
            sequential: self.sequential,
            period: self.period,
            breaks_on: self.breaks_on.clone(),
            trackers: self.trackers.iter().map(Tracker::clone_empty).collect(),
            state: TriggerState::Inactive,
        }
    }
}

/// A hook is a tuple of a trigger that determines when the hook activates, and an activator that determines
/// what happens when the hook activates. This tuple can itself be used as an element of the Stream (simple and
/// high performance), or it can be embedded in a `HookGroup` which happens when this hook is followed up by
/// a --withhold argument.
/// 
/// It is important that the Hook class doesn't do anything more than just behaving as a tuple of those two.
/// This is because the `HookGroup` class may bypass the functions of this class and interact with the Trigger
/// and HookActuator directly. Note that all members of this scruct are public.
pub struct Hook {
    /// The current state mutable at runtime.
    pub trigger: Trigger,

    /// The collection of all events and effects that may be caused by this hook.
    pub actuator: HookActuator,
}

impl Hook {
    pub fn new(trigger: Trigger, actuator: HookActuator) -> Hook {
        Hook { trigger, actuator }
    }

    fn apply(&mut self, event: Event, events_out: &mut Vec<Event>, state: &mut State, loopback: &mut LoopbackHandle) {
        // IMPORTANT: this function must NOT do anything more than just the following two lines of code!
        //
        // Other classes may assume that applying the hook to an event is equivalent to the following two function
        // calls, and interact with the trigger and the actuator directly, bypassing the Hook class.
        //
        // If any more logic were to be added to this function, then that logic would not be executed if this
        // hook becomes part of a `HookGroup`. Which is a bad thing.
        let response = self.trigger.apply(event, loopback);
        self.actuator.apply_response(response, event, (), events_out, state);
    }

    pub fn wakeup(&mut self, token: &loopback::Token) {
        self.trigger.wakeup(token);
    }

    pub fn apply_to_all(&mut self, events: &[Event], events_out: &mut Vec<Event>, state: &mut State, loopback: &mut LoopbackHandle) {
        for event in events {
            self.apply(*event, events_out, state, loopback);
        }
    }

    pub fn apply_to_all_caps(&self, caps: &[Capability], caps_out: &mut Vec<Capability>) {
        self.actuator.event_dispatcher.apply_to_all_caps(&self.trigger, caps, caps_out);
    }
}

pub struct HookActuator {
    /// Effects that shall be triggered if this hook activates, i.e. all keys are held down simultaneously.
    effects: Vec<Effect>,
    /// Effects that shall be released after one of the keys has been released after activating.
    release_effects: Vec<Effect>,

    /// The substructure responsible for generating additinal events for the send-key clause.
    event_dispatcher: EventDispatcher,
}

impl HookActuator {
    pub fn new(event_dispatcher: EventDispatcher) -> HookActuator {
        HookActuator {
            effects: Vec::new(),
            release_effects: Vec::new(),
            event_dispatcher,
        }
    }

    pub fn apply_response<T, U>(&mut self,
        response: TriggerResponse,
        event: Event,
        event_data: U,
        events_out: &mut T,
        state: &mut State
    ) where T: Sink<AdditionalData=U>
    {
        self.event_dispatcher.map_event(event, event_data, response, events_out);

        match response {
            TriggerResponse::Activates => {
                self.apply_effects(state);
            },
            TriggerResponse::Releases => {
                self.apply_release_effects(state);
            },
            TriggerResponse::Interacts | TriggerResponse::None => (),
        }
    }

    /// Runs all effects that should be ran when this hook triggers.
    fn apply_effects(&self, state: &mut State) {
        for effect in &self.effects {
            effect(state);
        }
    }

    /// Runs all effects that should be ran when this hook has triggered and
    /// a tracked key is released.
    fn apply_release_effects(&self, state: &mut State)
    {
        for release_effect in &self.release_effects {
            release_effect(state);
        }
    }

    /// Makes this hook run an effect when it triggers.
    pub fn add_effect(&mut self, effect: Effect) {
        self.effects.push(effect);
    }

    /// Makes this hook invoke an external subprocess when this hook is triggered.
    pub fn add_command(&mut self, program: String, args: Vec<String>) {
        self.add_effect(
            Box::new(move |_| {
                subprocess::try_spawn(program.clone(), args.clone()).print_err();
            })
        );
    }
}

/// The part of the --hook that is responsible for handling the send-key= clause.
/// Implemented separately from the hook because it is possible we want to remove this
/// functionality from the --hook itself and move it to a --withhold instead.
pub struct EventDispatcher {
    /// Events that shall be sent on press in the order specified.
    on_press: Vec<Key>,
    /// Events that shall be sent on release *in the order specified*. If you want them
    /// in another order, like reverse order, then reverse them before you put them here.
    on_release: Vec<Key>,
    /// The last event that activated the corresponding Hook/Trigger.
    activating_event: Option<Event>,
}

impl EventDispatcher {
    pub fn new(on_press: Vec<Key>, on_release: Vec<Key>) -> EventDispatcher {
        EventDispatcher {
            on_press, on_release,
            activating_event: None
        }
    }

    /// Similar in purpose to apply().
    fn map_event<T,U>(
        &mut self,
        // The event that is to be mapped.
        event: Event,
        // Data associated with the event to be mapped. When the mapped event is passed on to the sink,
        // this data shall be attached to the input event, and only the input event.
        event_data: U,
        // The response that was received when this event was given to the `Trigger`.
        trigger_response: TriggerResponse,
        // Where the original event and all generated events go.
        events_out: &mut T
    ) where T: Sink<AdditionalData = U>{
        match trigger_response {
            TriggerResponse::Activates => {
                events_out.push_event(event, event_data);
                self.activating_event = Some(event);
                for key in &self.on_press {
                    events_out.push_new_event(key.merge(event));
                };
            },
            TriggerResponse::Releases => {
                let activating_event = match self.activating_event {
                    Some(activating_event) => activating_event,
                    None => {
                        crate::utils::warn_once("Internal error: a hook released without record of being activated by any event. This is a bug.");
                        event
                    }
                };
                for key in &self.on_release {
                    events_out.push_new_event(key.merge(activating_event));
                }
                events_out.push_event(event, event_data);
            },
            TriggerResponse::Interacts | TriggerResponse::None => {
                events_out.push_event(event, event_data);
            },
        }
    }

    /// Like generate_additional_caps(), but also copies the input caps to the output.
    /// Needt to know which trigger is associated with this actuator to properly guess the caps.
    pub fn apply_to_all_caps(&self, trigger: &Trigger, caps: &[Capability], caps_out: &mut Vec<Capability>) {
        caps_out.extend(caps.iter().cloned());
        self.generate_additional_caps(trigger, caps, caps_out);
    }

    /// Computes additional capabilities that can be generated by the send_keys and writes them
    /// to caps_out. This function does not add the base capabilities to the output.
    /// 
    /// Similar in purpose to apply_to_all_caps(), but does not copy the base capabilities.
    fn generate_additional_caps(&self, trigger: &Trigger, caps: &[Capability], caps_out: &mut Vec<Capability>) {
        // TODO: LOW-PRIORITY Fix encapsulation?
        let keys: Vec<&Key> = trigger.trackers.iter().map(|tracker| &tracker.key).collect();
        let mut additional_caps: HashSet<Capability> = HashSet::new();
        // TODO: MEDIUM-PRIORITY reduce this implementation to a special case of Map.

        for cap_in in caps {
            // Find the values of this capability that might match any of the keys associated with the hook.
            let potentially_matching_values = keys.iter()
                .map(|key| key.matches_cap(cap_in))
                .fold(Set::empty(), |accumulator, (certainty, values)| {
                    // A compile-time assertion to make sure that these are the only two kinds of certainties
                    // that exist. Just in case I might add something like Certainty::Never later, which would
                    // break this function.
                    match certainty { Certainty::Always | Certainty::Maybe => () };

                    accumulator.union(&values)
                });
            
            if potentially_matching_values.is_empty() {
                continue;
            }
            let potentially_matching_cap = cap_in.clone().with_values(potentially_matching_values);

            let EventDispatcher { on_press, on_release, activating_event: _ } = self;
            let additional_events = on_press.iter().chain(on_release);
            additional_caps.extend(additional_events.map(
                |key| key.merge_cap(potentially_matching_cap.clone())
            ));
        }

        caps_out.extend(additional_caps);
    }
}

/// If this hook has a period set, acquires a Token from the loopback and arranges for a
/// `wakeup()` call later. If no period is set, return `ExpirationTime::Never`.
fn acquire_expiration_token(period: Option<Duration>, loopback: &mut LoopbackHandle) -> ExpirationTime {
    match period {
        Some(duration) => ExpirationTime::Until(loopback.schedule_wakeup_in(duration)),
        None => ExpirationTime::Never,
    }
}
