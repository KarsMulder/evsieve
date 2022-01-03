// SPDX-License-Identifier: GPL-2.0-or-later

use crate::error::Context;
use crate::range::Range;
use crate::key::Key;
use crate::state::{State};
use crate::event::Event;
use crate::subprocess;
use std::time::Instant;

pub type Effect = Box<dyn Fn(&mut State)>;

/// A tracker is used to track whether a certain key is held down. This is useful for --hook type
/// arguments.
struct Tracker {
    key: Key,
    range: Range,

    /// The state is mutable at runtime. It reflects whether the key tracked by this tracked
    /// is currently pressed or not.
    state: bool,
}

impl Tracker {
    // TODO: refactor this code duplication.
    fn new(mut key: Key) -> Tracker {
        let range = key.pop_value().unwrap_or_else(|| Range::new(Some(1), None));
        Tracker {
            key,
            range,
            state: false,
        }
    }

    /// If the event matches, remembers whether this event falls in the desired range.
    fn apply(&mut self, event: &Event) {
        if self.key.matches(event) {
            self.state = self.range.contains(event.value);
        }
    }

    fn is_down(&self) -> bool {
        self.state
    }
}

#[derive(Clone, Copy)]
enum PeriodTrackerState {
    /// This tracker's corresponding key is held down.
    /// This tracker remembers the last event that activated this tracker and when it was last activated.
    Active(Event, Instant),
    /// This tracker has been active and should withold the next key that would deactivate it.
    /// It no longer counts as active. If it is reactivated before it can uphold a key, the residual
    /// status expires early.
    Residual,
    /// This tracker's corresponding key is not held down.
    Inactive,
}

/// A tracker is used to track whether a certain key is held down. This is useful for --hook type
/// arguments.
struct PeriodTracker {
    key: Key,
    range: Range,

    /// The state is mutable at runtime. It reflects whether the key tracked by this tracked
    /// is currently pressed or not, as well as which event triggered it and when.
    state: PeriodTrackerState,
}

impl PeriodTracker {
    fn new(mut key: Key) -> Tracker {
        let range = key.pop_value().unwrap_or_else(|| Range::new(Some(1), None));
        Tracker {
            key,
            range,
            state: false,
        }
    }

    /// Returns true if this event might interact with this tracker in some way.
    fn matches(&self, event: &Event) -> bool {
        self.key.matches(event)
    }

    /// If the event matches, remembers whether this event falls in the desired range.
    fn apply(&mut self, event: &Event) {
        if self.key.matches(event) {
            self.state = match self.range.contains(event.value) {
                true =>  PeriodTrackerState::Active(*event, Instant::now()),
                false => PeriodTrackerState::Inactive
            }
        }
    }

    fn is_down(&self) -> bool {
        matches!(self.state, PeriodTrackerState::Active(_, _))
    }
}

#[derive(Clone, Copy)]
enum HookState {
    /// All trackers are currently pressed.
    Active,
    /// Not all trackers are currently pressed.
    Inactive,
}

struct PeriodHook {
    trackers: Vec<PeriodTracker>,
    state: HookState,
}

impl PeriodHook {
    fn apply(&mut self, event: Event, events_out: &mut Vec<Event>) {
        // ISSUE: Period hooks do not work if multiple keys can potentially overlap.
        if let Some(tracker) = self.trackers.iter_mut()
            .filter(|tracker| tracker.matches(&event))
            .next()
        {
            let new_state = match tracker.range.contains(event.value) {
                true => PeriodTrackerState::Active(event, Instant::now()),
                false => PeriodTrackerState::Inactive,
            };

            // If an event was upheld by this tracker, release it. Deactivate the tracker in the meanwhile.
            let previous_state = std::mem::replace(&mut tracker.state, new_state);
            if let PeriodTrackerState::Active(event, _) = previous_state {
                events_out.push(event);
            }

            match tracker.range.contains(event.value) {
                // If this tracker is activated by this event, uphold it.
                true => {}
                // If not, drop the event if this tracker was in residual state. Otherwise, drop it.
                false => {
                    match previous_state {
                        PeriodTrackerState::Residual => {},
                        PeriodTrackerState::Active(_, _) | PeriodTrackerState::Inactive
                            => events_out.push(event),
                    }
                }
            }
        } else {
            // No trackers care about this event.
            events_out.push(event);
        }
    }

    fn apply_to_all(&mut self, events: &[Event], events_out: &mut Vec<Event>) {
        for event in events {
            self.apply(*event, events_out);
        }
    }
}

pub struct Hook {
    hold_trackers: Vec<Tracker>,
    state: HookState,

    /// Effects that shall be triggered if this hook activates, i.e. all keys are held down simultaneously.
    effects: Vec<Effect>,
    /// Effects that shall be released after one of the keys has been released after activating.
    release_effects: Vec<Effect>,
}

impl Hook {
    pub fn new(hold_keys: Vec<Key>) -> Hook {
        let hold_trackers = hold_keys.into_iter().map(Tracker::new).collect();
        Hook {
            hold_trackers,
            state: HookState::Inactive,

            effects: Vec::new(),
            release_effects: Vec::new(),
        }
    }

    pub fn add_effect(&mut self, effect: Effect) {
        self.effects.push(effect);
    }

    fn apply(&mut self, event: &Event, state: &mut State) {
        for tracker in &mut self.hold_trackers {
            tracker.apply(event);
        }

        let all_trackers_down = self.hold_trackers.iter().all(Tracker::is_down);
        match (self.state, all_trackers_down) {
            (HookState::Active, false) => {
                self.state = HookState::Inactive;
                self.apply_release_effects(state);
            },
            (HookState::Inactive, true) => {
                self.state = HookState::Active;
                self.apply_effects(state);
            },
            (HookState::Active, true) => {},
            (HookState::Inactive, false) => {},
        }
    }

    fn apply_effects(&self, state: &mut State) {
        for effect in &self.effects {
            effect(state);
        }
    }

    fn apply_release_effects(&self, state: &mut State) {
        for release_effect in &self.release_effects {
            release_effect(state);
        }
    }

    pub fn apply_to_all(&mut self, events: &[Event], state: &mut State) {
        for event in events {
            self.apply(event, state);
        }
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