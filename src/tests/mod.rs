use crate::arguments::parser::PreImplementation;
use crate::event::{Event, EventCode, EventType, Namespace};
use crate::io::output::OutputSystem;
use crate::key::KeyParser;
use crate::stream::Setup;

struct VirtualOutputSystem {
    received_events: Vec<Event>,
}

impl VirtualOutputSystem {
    fn new() -> Self {
        Self {
            received_events: Vec::new(),
        }
    }
}

impl OutputSystem for &mut VirtualOutputSystem {
    fn update_caps(&mut self, _new_capabilities: Vec<crate::capability::Capability>) {
        // Do nothing.
    }

    fn route_events(&mut self, events: &[Event]) {
        self.received_events.extend_from_slice(events);
    }

    fn synchronize(&mut self) {
        // Do nothing.
    }
}

fn process_events(args: Vec<String>, events_in: Vec<Event>) -> Vec<Event> {
    let PreImplementation { stream, input_devices, output_devices, control_fifo_paths, state, toggle_indices } =
        crate::arguments::parser::process(args)
        .expect("Failed to process the arguments.");

    // Tests are not supposed to include any I/O devices.
    assert!(input_devices.is_empty());
    assert!(control_fifo_paths.is_empty());
    // They do however include an output device, which will not actually be created.
    let _ = output_devices;

    let mut output = VirtualOutputSystem::new();
    // TODO: Use more proper way of generating input capabilities?
    // Right now they're not used for anything but recreating output devices (which doesn't happen during tests),
    // but in the future they might get used for more.
    let input_capabilities = Default::default();
    let mut setup = Setup::create(stream, &mut output, state, toggle_indices, input_capabilities);
    run_stream(&mut setup, events_in);

    output.received_events
}

fn run_stream<T: OutputSystem>(setup: &mut Setup<T>, events_in: Vec<Event>) {
    let now = crate::time::Instant::now();
    for event in events_in {
        setup.wakeup_until(now);
        setup.run(now, event);
        setup.syn();
    }
}

/// For convenience we pass the arguments, input events and output events are all passed as a single string that will
/// be split by whitespace.
/// 
/// TODO: consider shellexing the string instead of splitting by whitespace.
fn run_test(args: &str, events_in: &str, events_out: &str) {
    let to_vec = |string: &str| string.split_whitespace().map(str::to_owned).collect::<Vec<String>>();
    let args: Vec<String> = to_vec(args);

    let prototype_event = Event::new(EventCode::new(EventType::KEY, 0), 0, 0, crate::domain::get_unique_domain(), Namespace::User);
    let keys_in  = KeyParser::default_mask().parse_all(&to_vec(events_in)).expect("Malformed input event.");
    let events_in: Vec<Event> = keys_in.into_iter().map(|key| key.merge(prototype_event)).collect();

    let keys_out_str = to_vec(events_out);
    let mut key_out_parser = KeyParser::default_filter();
    key_out_parser.namespace = Namespace::Output;
    let events_out = process_events(args, events_in);

    if keys_out_str.len() != events_out.len() {
        panic!("Wrong amount of events generated. Expected {} events, received {} events.", keys_out_str.len(), events_out.len());
    }

    for (event, key_str) in events_out.into_iter().zip(keys_out_str) {
        let key = key_out_parser.parse(&key_str).expect("Malformed output event.");
        if ! key.matches(&event) {
            panic!("Received unexpected event: {} instead of {key_str}", event);
        }
    }
}

#[test]
fn rudimentary_test() {
    run_test(
        "--map key:a key:b --output",
        "key:a:1 key:c:1 key:a:0 key:c:0",
        "key:b:1 key:c:1 key:b:0 key:c:0",
    )
}
