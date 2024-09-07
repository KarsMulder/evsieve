use crate::event::Event;

/// Most arguments take input arguments as a simple vector of events and push the output events
/// to another vector of events. This is simple and really fast.
/// 
/// However, sometimes we need to track some additional information about the created events.
/// Particularly, for --hook --withhold constructions, it is important to know which events
/// were used to trigger the hook and which are generated as consequence of the hook being
/// triggered.
/// 
/// The Sink trait is used as a zero-cost abstraction that will function as simply a vector in
/// the likely case that we don't care about the events' origin, but can be substituted for
/// something that does track the events' origin when needed.
pub trait Sink {
    fn push_created_event(&mut self, event: Event);
    fn push_retained_event(&mut self, event: Event);
}

pub enum EventCause {
    Created,
    Retained,
}

impl Sink for Vec<Event> {
    fn push_created_event(&mut self, event: Event) {
        self.push(event);
    }

    fn push_retained_event(&mut self, event: Event) {
        self.push(event);
    }
}

/// An abstract sink that calls arbitrary closures whenever it receives events.
pub struct AbstractSink<F1: FnMut(Event), F2: FnMut(Event)> {
    pub handle_created_event: F1,
    pub handle_retained_event: F2,
}

impl<F1: FnMut(Event), F2: FnMut(Event)> Sink for AbstractSink<F1, F2> {
    fn push_created_event(&mut self, event: Event) {
        (self.handle_created_event)(event);
    }

    fn push_retained_event(&mut self, event: Event) {
        (self.handle_retained_event)(event);
    }
}