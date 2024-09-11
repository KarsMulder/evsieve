use crate::event::Event;

/// Most arguments take input arguments as a simple vector of events and push the output events
/// to another vector of events. This is simple and really fast.
/// 
/// However, sometimes we need to track some additional information about the created events.
/// Information that is only relevant during a limited context and should not be copied to
/// other events that are generated based on maps or whatever. It would be a bad move to store
/// that information in the events themselves.
/// 
/// Particularly, for --hook --withhold constructions, it is important to know which events
/// were used to trigger the hook and which are generated as consequence of the hook being
/// triggered.
/// 
/// The Sink trait is used as a zero-cost abstraction that will function as simply a Vec<Event>
/// in the case that we don't track any additional information for events, but can be substituted
/// for something that does track additional information when needed.
/// 
/// When we're maybe tracking additional information, the apply() function is supposed to accept
/// two arguments: a &mut impl Sink, and another parameter that represents the information
/// associated with the input event. If this function puts the input event back into the event
/// stream as-is, it should call `push_event(event, additional_data)`. If it adds a created event
/// to the input stream, it should call `push_new_event(event)`.
pub trait Sink {
    /// Additional data associated with the incoming event. When that event gets retained,
    /// you should call `push_event()` along with the data you received together with the
    /// event. If you create a new event, you should use push that new event together with
    /// data from the `new_data()` function.
    type AdditionalData;

    /// Pushes an event together with additional data. Depending on the implementation of
    /// this Sink, the additional data may or may not be simply discarded.
    fn push_event(&mut self, event: Event, additional_data: Self::AdditionalData);

    /// Pushes a newly created event to this Sink, which has no additional data associated
    /// with it because it is new.
    fn push_new_event(&mut self, event: Event) {
        self.push_event(event, Self::new_data());
    }

    /// Returns the additional data that needs to be attached to a newly created event.
    fn new_data() -> Self::AdditionalData;
}

impl Sink for Vec<Event> {
    type AdditionalData = ();

    fn push_event(&mut self, event: Event, _additional_data: Self::AdditionalData) {
        self.push(event);
    }

    fn new_data() -> Self::AdditionalData {
        ()
    }
}

impl<T: Default> Sink for Vec<(Event, T)> {
    type AdditionalData = T;

    fn push_event(&mut self, event: Event, additional_data: Self::AdditionalData) {
        self.push((event, additional_data));
    }

    fn new_data() -> Self::AdditionalData {
        T::default()
    }
}