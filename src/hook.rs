use crate::error::Context;
use crate::range::Range;
use crate::key::Key;
use crate::event::Event;
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
enum ExpirationTime {
    Never,
    Until(loopback::Token),
}

/// TODO: ISSUE This does not work well with domains: it is possible that key:a@foo activates
/// it, and key:a@bar is then dropped by Residual. Fix this.
enum TrackerState {
    /// This tracker's corresponding key is held down.
    /// This tracker remembers the last event that activated this tracker and until when the tracker
    /// should stay active.
    Active(Event, ExpirationTime),
    /// The distinction between Active and Residual only matters if the hook has the `withhold`
    /// flag. (TODO: verify)
    ///
    /// This tracker has been active and should withold the next key that would deactivate it.
    /// It still counts as active, but no longer witholds a key. It can be re-activated to
    /// forget about witholding the key that should de-activate it.
    // TODO: Consider whether that behaviour is sensible.
    // TODO: Consider how this should interact with expiration.
    Residual,
    /// This tracker's corresponding key is not held down.
    Inactive,
    /// This tracker was active, but then expired and now can't become active again until a
    /// release event is encountered.
    Expired,
}

impl TrackerState {
    fn is_active(&self) -> bool {
        match self {
            TrackerState::Active (_, _) | TrackerState::Residual => true,
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

    /// Returns whether this event would turn this tracker on or off.
    /// Only returns sensible values if self.matches(event) is true.
    fn activates(&self, event: Event) -> bool {
        self.range.contains(event.value)
    }
}

#[derive(Clone, Copy)]
enum HookState {
    /// All trackers are currently pressed.
    /// Remembers the event that activated this hook.
    Active { activating_event: Event },
    /// Not all trackers are currently pressed.
    Inactive,
}

pub struct Hook {
    /// Options that can be configured by the user.
    withhold: bool,
    period: Option<Duration>, // TODO: unimplemented.

    /// Effects that shall be triggered if this hook activates, i.e. all keys are held down simultaneously.
    effects: Vec<Effect>,
    /// Effects that shall be released after one of the keys has been released after activating.
    release_effects: Vec<Effect>,
    /// Keys that shall be sent on press and release.
    /// TODO: Consider integrating this with the effect system.
    send_keys: Vec<Key>,

    /// The current state mutable at runtime.
    trackers: Vec<Tracker>,
    state: HookState,
}

impl Hook {
    pub fn new(keys: Vec<Key>, withhold: bool, period: Option<Duration>) -> Hook {
        let trackers = keys.into_iter().map(Tracker::new).collect();
        Hook {
            withhold,
            period,

            trackers,
            state: HookState::Inactive,

            effects: Vec::new(),
            release_effects: Vec::new(),
            send_keys: Vec::new(),
        }
    }

    /// Returns true is any tracker might be affected by the provided event.
    /// This function does not affect the tracker state and is not affected by the tracker state.
    pub fn matches(&self, event: Event) -> bool {
        self.trackers.iter().any(|tracker| tracker.matches(&event))
    }

    fn apply(&mut self, event: Event, events_out: &mut Vec<Event>, state: &mut State, loopback: &mut LoopbackHandle) {
        let mut any_tracker_matched: bool = false;
        for tracker in self.trackers.iter_mut()
            .filter(|tracker| tracker.matches(&event))
        {
            any_tracker_matched = true;

            let new_state = if tracker.activates(event) {
                // Put a temporary dummy value in tracker.state. We will soon put
                // a sensible value back.
                match std::mem::replace(&mut tracker.state, TrackerState::Inactive) {
                    active @ TrackerState::Active(..) => active,
                    TrackerState::Inactive => {
                        TrackerState::Active(
                            event, acquire_expiration_token(self.period, loopback)
                        )
                    },
                    TrackerState::Residual => TrackerState::Residual,
                    TrackerState::Expired => TrackerState::Expired,
                }
            } else {
                TrackerState::Inactive
            };
            let previous_state = std::mem::replace(&mut tracker.state, new_state);

            // If this hook does not withold events, write the event out and carry on.
            // Otherwise, do some complex logic to determine if this events needs to be
            // withheld and/or old events must be released.
            if ! self.withhold {
                events_out.push(event);
                continue;
            }
            
            match tracker.activates(event) {
                // If this tracker is activated by this event, withhold it.
                true => {},
                false => {
                    match previous_state {
                        TrackerState::Residual => {},
                        // If an event was previously held up, release it together with
                        // the new event.
                        TrackerState::Active(old_event, _expiration) => {
                            events_out.push(old_event);
                            events_out.push(event);
                        },
                        TrackerState::Inactive | TrackerState::Expired => {
                            events_out.push(event);
                        },
                    }
                },
            }

            // ISSUE: withholding hooks do not work if multiple keys can potentially overlap.
            break;
        }
        
        if ! any_tracker_matched {
            // No trackers care about this event.
            events_out.push(event);
            return;
        }

        // Check if we transitioned between active and inactive.
        let all_trackers_active = self.trackers.iter().all(|tracker| tracker.state.is_active());

        match (self.state, all_trackers_active) {
            (HookState::Inactive, true) => {
                self.state = HookState::Active { activating_event: event };
                for tracker in &mut self.trackers {
                    tracker.state = TrackerState::Residual;
                }
                self.apply_effects(event, events_out, state);
            },
            (HookState::Active { activating_event }, false) => {
                self.state = HookState::Inactive;
                self.apply_release_effects(activating_event, events_out, state);
            },
            (HookState::Active {..}, true) | (HookState::Inactive, false) => {},
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
            let keys: Vec<&Key> = self.trackers.iter().map(|tracker| &tracker.key).collect();
            generate_additional_caps(&keys, &self.send_keys, caps, caps_out);
        }
    }

    pub fn wakeup(&mut self, token: &loopback::Token, events_out: &mut Vec<Event>) {
        // TODO: try maintaining weak ordering when releasing events?
        // Release all events that have expired.
        for tracker in &mut self.trackers {
            match tracker.state {
                TrackerState::Active(_event, ExpirationTime::Never) => {},
                TrackerState::Residual => {}, // TODO: handle expiration of residual.
                TrackerState::Inactive => {},
                TrackerState::Expired => {},
                TrackerState::Active(event, ExpirationTime::Until(ref other_token)) => {
                    if token == other_token {
                        tracker.state = TrackerState::Expired;
                        if self.withhold {
                            events_out.push(event);
                        }
                    }
                }
            }
        }
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
