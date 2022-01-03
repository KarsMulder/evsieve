use crate::error::Context;
use crate::range::Range;
use crate::key::Key;
use crate::event::Event;
use std::time::{Instant, Duration};

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
    /// This tracker has been active and should withold the next key that would deactivate it.
    /// It still counts as active, but no longer witholds a key. It can be re-activated to
    /// forget about witholding the key that should de-activate it.
    // TODO: Consider whether that behaviour is sensible.
    Residual,
    /// This tracker's corresponding key is not held down.
    Inactive,
}

impl TrackerState {
    fn is_active(&self) -> bool {
        match self {
            TrackerState::Active (_, _) | TrackerState::Residual => true,
            TrackerState::Inactive => false,
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

    fn is_down(&self) -> bool {
        matches!(self.state, TrackerState::Active(_, _))
    }
}

#[derive(Clone, Copy)]
enum HookState {
    /// All trackers are currently pressed.
    Active,
    /// Not all trackers are currently pressed.
    Inactive,
}

struct Hook {
    withhold: bool,
    period: Option<Duration>,

    trackers: Vec<Tracker>,
    state: HookState,
}

impl Hook {
    fn apply(&mut self, event: Event, events_out: &mut Vec<Event>) {
        let mut any_tracker_matched: bool = false;
        for tracker in self.trackers.iter_mut()
            .filter(|tracker| tracker.matches(&event))
        {
            any_tracker_matched = true;

            let new_state = match tracker.activates(event) {
                true => TrackerState::Active(event, ExpirationTime::Never),
                false => TrackerState::Inactive,
            };
            let previous_state = std::mem::replace(&mut tracker.state, new_state);

            // If this hook does not withold events, write the event out and carry on.
            // Otherwise, do some complex logic to determine if this events needs to be
            // withheld and/or old events must be released.
            if ! self.withhold {
                events_out.push(event);
                continue;
            }

            // If an event was upheld by this tracker, release it.
            if let TrackerState::Active(old_event, _) = previous_state {
                events_out.push(old_event);
            }
            
            match tracker.activates(event) {
                // If this tracker is activated by this event, withhold it.
                true => {}
                // If not, drop the event if this tracker was in residual state.
                false => {
                    match previous_state {
                        TrackerState::Residual => {},
                        TrackerState::Active(_, _) | TrackerState::Inactive
                            => events_out.push(event),
                    }
                }
            }

            // ISSUE: withholding hooks do not work if multiple keys can potentially overlap.
            break;
        }
        
        if ! any_tracker_matched {
            // No trackers care about this event.
            events_out.push(event);
            return;
        }

        let all_trackers_active = self.trackers.iter().all(|tracker| tracker.state.is_active());

        match (self.state, all_trackers_active) {
            (HookState::Inactive, true) => {
                // The tracker activates.
                self.state = HookState::Active;
                for tracker in &mut self.trackers {
                    tracker.state = TrackerState::Residual;
                }
                // TODO
            },
            (HookState::Active, false) => {
                self.state = HookState::Inactive;
                // TODO
            },
            (HookState::Active, true) | (HookState::Inactive, false) => {},
        }
    }

    fn apply_to_all(&mut self, events: &[Event], events_out: &mut Vec<Event>) {
        for event in events {
            self.apply(*event, events_out);
        }
    }
}