use crate::arguments::parser::PreImplementation;
use crate::event::{Event, EventCode, EventType, Namespace};
use crate::io::output::OutputSystem;
use crate::key::KeyParser;
use crate::stream::Setup;
use std::fmt::Write;

/// A replacement for the UInputSystem that does not actually write any events to any event devices,
/// but instead keeps track of all events it received.
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

struct EventPairResult<'a> {
    expected: Option<&'a str>,
    received: Option<Event>,
    matches: bool,
}

/// For convenience we pass the arguments, input events and output events are all passed as a single string that will
/// be split by whitespace. No --input or --output argument needs to be present.
/// 
/// TODO: consider shellexing the string instead of splitting by whitespace.
pub fn run_test(args: &str, events_in: &str, events_out: &str) {
    let to_vec = |string: &str| string.split_whitespace().filter(|x| !x.is_empty()).map(str::to_owned).collect::<Vec<String>>();
    let args: Vec<String> = to_vec(args);

    let prototype_event = Event::new(EventCode::new(EventType::KEY, 0), 0, 0, crate::domain::get_unique_domain(), Namespace::User);
    let keys_in  = KeyParser::default_mask().parse_all(&to_vec(events_in)).expect("Malformed input event.");
    let events_in: Vec<Event> = keys_in.into_iter().map(|key| key.merge(prototype_event)).collect();

    let keys_out_str = to_vec(events_out);
    let key_out_parser = KeyParser::default_filter();
    let events_out = process_events(args, events_in);
    let mut result: Vec<EventPairResult> = Vec::new();

    for i in 0 .. usize::max(events_out.len(), keys_out_str.len()) {
        let event = events_out.get(i);
        let key_str = keys_out_str.get(i);
        let key = key_str.map(|x| key_out_parser.parse(x).expect("Malformed output event"));
        let matches = match (event, key) {
            (Some(event), Some(key)) => key.matches(event),
            _ => false,
        };

        result.push(EventPairResult {
            expected: key_str.map(String::as_str),
            received: event.copied(),
            matches
        });
    }

    if ! result.iter().all(|x| x.matches) {
        let report = create_report(&result);
        panic!("{}", report);
    }
}

fn create_report(results: &[EventPairResult]) -> String {
    let mut report = String::new();
    writeln!(report, " {:<20}| {}", "Expected", "Received").unwrap();
    writeln!(report, "{}", "-".repeat(50)).unwrap();
    for res in results {
        let expected = match res.expected {
            Some(key) => key.to_string(),
            None => "(none)".to_string(),
        };
        let received = match res.received {
            Some(event) => event.to_string(),
            None => "(none)".to_string(),
        };
        let status_indicator = match res.matches {
            true => "",
            false => "[!]",
        };
        
        writeln!(report, " {:<20}| {:<20} {}", expected, received, status_indicator).unwrap();
    }
    report
}
