use crate::error::Context;
use crate::range::Range;
use crate::key::Key;
use crate::event::{Event, Channel};
use crate::state::State;
use crate::subprocess;
use crate::loopback;
use crate::loopback::LoopbackHandle;
use crate::capability::{Capability, CapMatch};
use std::time::Duration;
use std::collections::HashSet;

pub type Effect = Box<dyn Fn(&mut State)>;

/// Represents the point at time after which a pressed tracker is no longer valid.
/// Usually determined by the --hook period= clause.
pub enum ExpirationTime {
    Never,
    Until(loopback::Token),
}

enum TrackerState {
    /// This tracker's corresponding key is held down.
    /// This tracker remembers the last event that activated this tracker and until when the tracker
    /// should stay active.
    Active(ExpirationTime),
    /// This tracker's corresponding key is not held down.
    Inactive,
    /// This tracker was active, but then expired and now can't become active again until a
    /// release event is encountered.
    Expired,
}

impl TrackerState {
    fn is_active(&self) -> bool {
        match self {
            TrackerState::Active (_) => true,
            TrackerState::Inactive | TrackerState::Expired => false,
        }
    }
}

/// A tracker is used to track whether a certain key is held down. This is useful for --hook type
/// arguments.
struct Tracker {
    key: Key,
    range: Range,

    /// The state is mutable at runtime. It reflects whether the key tracked by this tracked
    /// is currently pressed or not, as well as which event triggered it and when.
    state: TrackerState,
}

impl Tracker {
    fn new(mut key: Key) -> Tracker {
        let range = key.pop_value().unwrap_or_else(|| Range::new(Some(1), None));
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
            TrackerState::Expired | TrackerState::Inactive => false,
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

    trackers: Vec<Tracker>,
    state: TriggerState,
}

/// Returned by Trigger::apply to inform the caller what effect the provided event had on
/// the hook.
pub enum TriggerResponse {
    /// This event does not interact with this hook in any way.
    None,
    /// This event matches the key one of the trackers. Does not guarantee that the actual
    /// state of the tracker was changed.
    Matches,
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
    pub fn new(keys: Vec<Key>, period: Option<Duration>) -> Trigger {
        let trackers = keys.into_iter().map(Tracker::new).collect();
        Trigger {
            period, trackers,
            state: TriggerState::Inactive,
        }
    }

    pub fn apply(&mut self, event: Event, loopback: &mut LoopbackHandle) -> TriggerResponse {
        let mut any_tracker_matched: bool = false;
        for tracker in self.trackers.iter_mut()
            .filter(|tracker| tracker.matches(&event))
        {
            any_tracker_matched = true;

            // TODO: Refactor?
            if tracker.activates_by(event) {
                tracker.state = match std::mem::replace(&mut tracker.state, TrackerState::Inactive) {
                    active @ TrackerState::Active(..) => active,
                    TrackerState::Inactive => {
                        TrackerState::Active(
                            acquire_expiration_token(self.period, loopback)
                        )
                    },
                    TrackerState::Expired => TrackerState::Expired,
                }
            } else {
                tracker.state = TrackerState::Inactive;
            };
        }
        
        if ! any_tracker_matched {
            // No trackers care about this event.
            return TriggerResponse::None;
        }

        // Check if we transitioned between active and inactive.
        let all_trackers_active = self.trackers.iter().all(|tracker| tracker.state.is_active());

        match (self.state, all_trackers_active) {
            (TriggerState::Inactive, true) => {
                self.state = TriggerState::Active;
                // TODO: Cancel tokens?
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
                => TriggerResponse::Matches,
        }
    }

    /// Release a tracker that has expired. If a tracker expired, returns the associated key.
    /// It is important that the Tokens are unique for this function to work correctly.
    pub fn wakeup(&mut self, token: &loopback::Token) {
        for tracker in &mut self.trackers {
            match tracker.state {
                TrackerState::Inactive => {},
                TrackerState::Expired => {},
                TrackerState::Active(ExpirationTime::Never) => {},
                TrackerState::Active(ExpirationTime::Until(ref other_token)) => {
                    if token == other_token {
                        tracker.state = TrackerState::Expired;
                    }
                }
            }
        }
    }

    /// Returns true if any of the active trackers might have been activated by an event
    /// with the provided channel, regardless of whether that channel actually activated them.
    pub fn has_active_tracker_matching_channel(&self, channel: Channel) -> bool {
        self.trackers.iter()
            .filter(|tracker| tracker.is_active())
            .any(   |tracker| tracker.matches_channel(channel))
    }

    /// Like Clone::clone, but does not clone the runtime state of the Trigger.
    pub fn clone_empty(&self) -> Trigger {
        Trigger {
            period: self.period,
            trackers: self.trackers.iter().map(Tracker::clone_empty).collect(),
            state: TriggerState::Inactive,
        }
    }
}

pub struct Hook {
    /// Effects that shall be triggered if this hook activates, i.e. all keys are held down simultaneously.
    effects: Vec<Effect>,
    /// Effects that shall be released after one of the keys has been released after activating.
    release_effects: Vec<Effect>,
    /// Keys that shall be sent on press and release.
    /// TODO: Consider integrating this with the effect system.
    send_keys: Vec<Key>,

    /// The current state mutable at runtime.
    trigger: Trigger,
    /// The last event that activated this tracker.
    activating_event: Option<Event>,
}

impl Hook {
    pub fn new(trigger: Trigger) -> Hook {
        Hook {
            trigger,
            effects: Vec::new(),
            release_effects: Vec::new(),
            send_keys: Vec::new(),
            activating_event: None,
        }
    }

    fn apply(&mut self, event: Event, events_out: &mut Vec<Event>, state: &mut State, loopback: &mut LoopbackHandle) {
        events_out.push(event);
        let response = self.trigger.apply(event, loopback);
        match response {
            TriggerResponse::Activates => {
                self.activating_event = Some(event);
                self.apply_effects(event, events_out, state);
            },
            TriggerResponse::Releases => {
                let activating_event = self.activating_event.take().unwrap_or_else(|| {
                    crate::utils::warn_once("Internal error: a hook released while its activating_event is None.");
                    event // The sanest default I can think of.
                });
                self.apply_release_effects(activating_event, events_out, state);
            },
            TriggerResponse::Matches | TriggerResponse::None => (),
        }
    }

    pub fn apply_to_all(
        &mut self,
        events: &[Event],
        events_out: &mut Vec<Event>,
        state: &mut State,
        loopback: &mut LoopbackHandle,
    ) {
        for event in events {
            self.apply(*event, events_out, state, loopback);
        }
    }

    pub fn apply_to_all_caps(
        &self,
        caps: &[Capability],
        caps_out: &mut Vec<Capability>,
    ) {
        caps_out.extend(caps);
        if ! self.send_keys.is_empty() {
            // TODO: This is bad encapsulation. Refactor.
            let keys: Vec<&Key> = self.trigger.trackers.iter().map(|tracker| &tracker.key).collect();
            generate_additional_caps(&keys, &self.send_keys, caps, caps_out);
        }
    }

    pub fn wakeup(&mut self, token: &loopback::Token) {
        self.trigger.wakeup(token);
    }

    /// Runs all effects that should be ran when this hook triggers.
    fn apply_effects(
            &self, activating_event: Event, events_out: &mut Vec<Event>, state: &mut State
    ) {
        for effect in &self.effects {
            effect(state);
        }
        // TODO: Consider integrating this special call into the Effect system.
        send_keys_press(
            &self.send_keys,
            activating_event,
            events_out,
        )
    }

    /// Runs all effects that should be ran when this hook has triggered and
    /// a tracked key is released.
    /// 
    /// IMPORTANT: activating_event is the event that activated the hook, not the event that
    /// caused it to be released.
    fn apply_release_effects(
            &self, activating_event: Event, events_out: &mut Vec<Event>, state: &mut State)
    {
        for release_effect in &self.release_effects {
            release_effect(state);
        }
        // TODO: Consider integrating this special call into the Effect system.
        send_keys_release(
            &self.send_keys,
            activating_event,
            events_out,
        )
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

    /// Adds an effect: send a KEY_DOWN event of the provided key when the hook activates,
    /// and a KEY_UP event of the provided key when the hook releases.
    pub fn add_send_key(&mut self, key: Key) {
        self.send_keys.push(key);
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

/// Computes additional capabilities that can be generated by the send_keys and writes them
/// to caps_out. This function does not add the base capabilities to the output.
/// 
/// trigger_keys must be a list of all keys that can possibly be used as base to merge
/// send_keys with.
fn generate_additional_caps(
        trigger_keys: &[&Key], send_keys: &[Key],
        caps_in: &[Capability], caps_out: &mut Vec<Capability>)
{
    // TODO: write unittest for this function.
    let mut additional_caps: HashSet<Capability> = HashSet::new();

    for cap_in in caps_in {
        let matches_cap = trigger_keys.iter()
            .map(|key| key.matches_cap(cap_in)).max();
        match matches_cap {
            Some(CapMatch::Yes | CapMatch::Maybe) => {},
            Some(CapMatch::No) | None => continue,
        };

        additional_caps.extend(send_keys.iter().map(
            |key| {
                let mut new_cap = key.merge_cap(*cap_in);
                new_cap.value_range = Range::new(Some(0), Some(1));
                new_cap
            }
        ));
    }

    caps_out.extend(additional_caps);
}

fn send_keys_press(
    send_keys: &[Key],
    activating_event: Event,
    events_out: &mut Vec<Event>)
{
    for key in send_keys {
        let mut event = key.merge(activating_event);
        event.value = 1;
        events_out.push(event);
    }
}

/// IMPORTANT: activating_event must be the event that caused the hook to ACTIVATE, not the 
/// event that deactivated it.
fn send_keys_release(
    send_keys: &[Key],
    activating_event: Event,
    events_out: &mut Vec<Event>)
{
    for key in send_keys {
        let mut event = key.merge(activating_event);
        event.value = 0;
        events_out.push(event);
    }
}
