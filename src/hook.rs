use crate::error::Context;
use crate::range::Range;
use crate::key::Key;
use crate::event::Event;
use crate::state::State;
use crate::subprocess;
use crate::loopback::Loopback;
use crate::capability::Capability;
use std::time::{Instant, Duration};

pub type Effect = Box<dyn Fn(&mut State)>;

/// Represents the point at time after which a pressed tracker is no longer valid.
/// Usually determined by the --hook period= clause.
#[derive(Clone, Copy)]
enum ExpirationTime {
    Never,
    Until(Instant),
}

#[derive(Clone, Copy)]
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
    /// Additional capabilities that this hooks' effects can generate.
    additional_caps: Vec<Capability>,

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
            additional_caps: Vec::new(),
        }
    }

    fn apply(&mut self, event: Event, events_out: &mut Vec<Event>, state: &mut State, loopback: &mut Loopback) {
        let mut any_tracker_matched: bool = false;
        for tracker in self.trackers.iter_mut()
            .filter(|tracker| tracker.matches(&event))
        {
            any_tracker_matched = true;

            let expiration_time = get_expiration_time_of_new_event(self.period);
            let new_state = if tracker.activates(event) {
                match tracker.state {
                    active @ TrackerState::Active(..) => active,
                    TrackerState::Inactive | TrackerState::Residual => {
                        TrackerState::Active(event, expiration_time)
                    },
                    TrackerState::Expired => TrackerState::Expired,
                }
            } else {
                TrackerState::Inactive
            };
            let previous_state = std::mem::replace(&mut tracker.state, new_state);

            // If this event expires, ask the loopback device to wake us up when it does.
            match expiration_time {
                ExpirationTime::Never => {},
                ExpirationTime::Until(time) => loopback.schedule_wakeup(time),
            }

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

        match (self.is_active(), all_trackers_active) {
            (false, true) => {
                self.state = HookState::Active { activating_event: event };
                for tracker in &mut self.trackers {
                    tracker.state = TrackerState::Residual;
                }
                self.apply_effects(state);
            },
            (true, false) => {
                self.state = HookState::Inactive;
                self.apply_release_effects(state);
            },
            (true, true) | (false, false) => {},
        }
    }

    fn is_active(&self) -> bool {
        matches!(self.state, HookState::Active { .. })
    }

    pub fn apply_to_all(
        &mut self,
        events: &[Event],
        events_out: &mut Vec<Event>,
        state: &mut State,
        loopback: &mut Loopback,
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
        caps_out.extend(&self.additional_caps);
    }

    pub fn wakeup(&mut self, now: Instant, events_out: &mut Vec<Event>) {
        // TODO: try maintaining weak ordering when releasing events?
        // Release all events that have expired.
        for tracker in &mut self.trackers {
            match tracker.state {
                TrackerState::Active(_event, ExpirationTime::Never) => {},
                TrackerState::Residual => {}, // TODO: handle expiration of residual.
                TrackerState::Inactive => {},
                TrackerState::Expired => {},
                TrackerState::Active(event, ExpirationTime::Until(time)) => {
                    if time <= now {
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
    fn apply_effects(&self, state: &mut State) {
        for effect in &self.effects {
            effect(state);
        }
    }

    /// Runs all effects that should be ran when this hook has triggered and
    /// a tracked key is released.
    fn apply_release_effects(&self, state: &mut State) {
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

/// Returns the expiration time of a new event that would activate a tracker given
/// the period of a hook.
fn get_expiration_time_of_new_event(period: Option<Duration>) -> ExpirationTime {
    match period {
        Some(duration) => {
            let now = Instant::now(); // TODO: consider caching this result?
            ExpirationTime::Until(now + duration)
        },
        None => ExpirationTime::Never,
    }
}