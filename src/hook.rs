// SPDX-License-Identifier: GPL-2.0-or-later

use crate::error::Context;
use crate::range::Range;
use crate::key::Key;
use crate::state::{State};
use crate::event::Event;
use crate::subprocess;

pub type Effect = Box<dyn Fn(&mut State)>;

#[derive(Clone, Copy)]
enum TrackerState {
    /// This tracker's corresponding key is held down.
    /// This tracker remembers the last event that activated this tracker.
    Active(Event),
    /// This tracker's corresponding key is not held down.
    Inactive,
}

/// A tracker is used to track whether a certain key is held down. This is useful for --hook type
/// arguments.
struct Tracker {
    key: Key,
    range: Range,

    /// The state is mutable at runtime. It reflects whether the key tracked by this tracked
    /// is currently pressed or not.
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

    /// If the event matches, remembers whether this event falls in the desired range.
    fn apply(&mut self, event: &Event) {
        if self.key.matches(event) {
            self.state = match self.range.contains(event.value) {
                true => TrackerState::Active(*event),
                false => TrackerState::Inactive,
            }
        }
    }

    fn is_down(&self) -> bool {
        matches!(self.state, TrackerState::Active(_))
    }
}

#[derive(Clone, Copy)]
enum HookState {
    /// All trackers are currently pressed.
    Active,
    /// Not all trackers are currently pressed.
    Inactive,
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