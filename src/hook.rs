// SPDX-License-Identifier: GPL-2.0-or-later

use crate::error::Context;
use crate::range::Range;
use crate::key::Key;
use crate::state::{State};
use crate::event::Event;
use crate::subprocess;

pub type Effect = Box<dyn Fn(&mut State)>;

/// A tracker is used to track whether a certain key is held down. This is useful for --hook type
/// arguments.
#[derive(Debug)]
struct Tracker {
    key: Key,
    range: Range,

    /// The state is mutable at runtime. It reflects whether the key tracked by this tracked
    /// is currently pressed or not.
    state: bool,
}

impl Tracker {
    fn new(mut key: Key) -> Tracker {
        let range = key.pop_value().unwrap_or_else(|| Range::new(Some(1), None));
        Tracker {
            key,
            range,
            state: false,
        }
    }

    /// If the event matches, remembers whether this event falls in the desired range.
    /// If this event falls in the desired range and the previous one didn't, returns true.
    /// Otherwise, returns false.
    fn apply(&mut self, event: &Event) -> bool {
        if self.key.matches(event) {
            let previous_value = self.state;
            let new_value = self.range.contains(event.value);
            self.state = new_value;
            
            new_value && ! previous_value
        } else {
            false
        }
    }

    fn is_down(&self) -> bool {
        self.state
    }
}

pub struct Hook {
    hold_trackers: Vec<Tracker>,
    effects: Vec<Effect>,
}

impl Hook {
    pub fn new(hold_keys: Vec<Key>) -> Hook {
        let hold_trackers = hold_keys.into_iter().map(Tracker::new).collect();
        Hook { hold_trackers, effects: Vec::new() }
    }

    pub fn add_effect(&mut self, effect: Effect) {
        self.effects.push(effect);
    }

    fn apply(&mut self, event: &Event, state: &mut State) {
        let any_tracker_activated = self.hold_trackers.iter_mut().any(
            |tracker| tracker.apply(event)
        );

        // Check whether at least one tracker turned active that wasn't on active,
        // i.e. whether this event contributed to the filters of this hook.
        if ! any_tracker_activated {
            return;
        }

        // Test whether all other trackers are active.
        for tracker in &self.hold_trackers {
            if ! tracker.is_down() {
                return;
            }
        }
        self.apply_effects(state);
    }

    fn apply_effects(&self, state: &mut State) {
        for effect in &self.effects {
            effect(state);
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