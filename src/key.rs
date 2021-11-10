// SPDX-License-Identifier: GPL-2.0-or-later

use crate::domain;
use crate::domain::Domain;
use crate::event::{Event, EventType, Namespace, VirtualEventType};
use crate::utils;
use crate::error::ArgumentError;
use crate::capability::{Capability, CapMatch};
use crate::range::Range;
use crate::ecodes;
use crate::event::EventCode;
use crate::error::Context;

#[derive(Clone, Debug)]
pub struct Key {
    properties: Vec<KeyProperty>,
}

impl Key {
    fn new() -> Key {
        Key {
            properties: Vec::new()
        }
    }

    /// Generates a key with no properties. Identical to Key::new(), but semantically signifies
    /// that you intend to use this key to generate identical copies of events, such as for the
    /// purpose of implementing --copy. You should not use this function if you intend to add more
    /// properties to a key.
    pub fn copy() -> Key {
        Key::new()
    }

    /// Returns a key that matches all events with a certain domain and namespace.
    pub fn from_domain_and_namespace(domain: Domain, namespace: Namespace) -> Key {
        let mut result = Key::new();
        result.add_property(KeyProperty::Namespace(namespace));
        result.add_property(KeyProperty::Domain(domain));
        result
    }

    pub fn matches(&self, event: &Event) -> bool {
        self.properties.iter().all(|prop| prop.matches(event))
    }

    /// Returns Yes if this key will guaranteedly match any event said Capability might emit,
    /// returns No if it cannot possibly match any such event, and otherwise Maybe.
    pub fn matches_cap(&self, cap: &Capability) -> CapMatch {
        self.properties.iter().map(|prop| prop.matches_cap(cap)).min().unwrap_or(CapMatch::Yes)
    }

    pub fn merge(&self, mut event: Event) -> Event {
        for prop in &self.properties {
            event = prop.merge(event);
        }
        event
    }

    pub fn merge_cap(&self, mut cap: Capability) -> Capability {
        for prop in &self.properties {
            cap = prop.merge_cap(cap);
        }
        cap
    }

    /// If this key has a `Value` property, returns the range of said property and removes
    /// it from its own properties. This is useful to decouple the key being matched upon
    /// from the value being matched upon.
    /// 
    /// The result is undefined if this key has multiple copies of a `Value` property.
    pub fn pop_value(&mut self) -> Option<Range> {
        let mut result: Option<Range> = None;
        self.properties.retain(
            |&property| {
                match property {
                    KeyProperty::Value(range) => {
                        result = Some(range);
                        false
                    },
                    _ => true,
                }
            }
        );
        result
    }

    fn add_property(&mut self, property: KeyProperty) {
        self.properties.push(property);
    }
}

#[derive(Clone, Copy, Debug)]
enum KeyProperty {
    Code(EventCode),
    Domain(Domain),
    VirtualType(VirtualEventType),
    Namespace(Namespace),
    Value(Range),
    PreviousValue(Range),
}

impl KeyProperty {
    /// Checkes whether an event matches this KeyProperty.
    pub fn matches(&self, event: &Event) -> bool {
        match *self {
            KeyProperty::Code(value) => event.code == value,
            KeyProperty::Domain(value) => event.domain == value,
            KeyProperty::VirtualType(value) => event.code.virtual_ev_type() == value,
            KeyProperty::Namespace(value) => event.namespace == value,
            KeyProperty::Value(range) => range.contains(event.value),
            KeyProperty::PreviousValue(range) => range.contains(event.previous_value),
        }
    }

    /// Given an Event, will return the closest event that matches this KeyProperty.
    pub fn merge(&self, mut event: Event) -> Event {
        match *self {
            KeyProperty::Code(value) => event.code = value,
            KeyProperty::Domain(value) => event.domain = value,
            KeyProperty::Namespace(value) => event.namespace = value,
            KeyProperty::Value(range) => event.value = range.bound(event.value),
            KeyProperty::PreviousValue(range) => event.previous_value = range.bound(event.previous_value),
            KeyProperty::VirtualType(_) => {
                if cfg!(debug_assertions) {
                    panic!("Cannot change the event type of an event. Panicked during event mapping.");
                } else {
                    utils::warn_once("ERROR: cannot change the event type of an event. If you see this message, this is a bug.");
                }
            }
        };
        event
    }

    pub fn matches_cap(&self, cap: &Capability) -> CapMatch {
        match *self {
            KeyProperty::Code(value) => (cap.code == value).into(),
            KeyProperty::Domain(value) => (cap.domain == value).into(),
            KeyProperty::VirtualType(value) => (cap.code.virtual_ev_type() == value).into(),
            KeyProperty::Namespace(value) => (cap.namespace == value).into(),
            KeyProperty::Value(range) => {
                if cap.value_range.is_subset_of(&range) {
                    CapMatch::Yes
                } else if range.is_disjoint_with(&cap.value_range) {
                    CapMatch::No
                } else {
                    CapMatch::Maybe
                }
            },
            KeyProperty::PreviousValue(_range) => CapMatch::Maybe,
        }
    }

    pub fn merge_cap(&self, mut cap: Capability) -> Capability {
        match *self {
            KeyProperty::Code(value) => cap.code = value,
            KeyProperty::Domain(value) => cap.domain = value,
            KeyProperty::Namespace(value) => cap.namespace = value,
            KeyProperty::Value(range) => cap.value_range = range.bound_range(&cap.value_range),
            KeyProperty::PreviousValue(_range) => {},
            KeyProperty::VirtualType(_) => {
                if cfg!(debug_assertions) {
                    panic!("Cannot change the event type of an event. Panicked during capability propagation.");
                } else {
                    utils::warn_once("ERROR: cannot change the event type of an event. If you see this message, this is a bug.");
                }
            }
        };
        cap
    }
}

/// Represents the options for how a key can be parsed in different contexts.
#[allow(non_snake_case)]
pub struct KeyParser<'a> {
    pub default_value: &'a str,
    pub allow_transitions: bool,
    pub allow_ranges: bool,
    pub allow_types: bool,
    /// If this true, then only event types of type EV_KEY (i.e. key:something or btn:something) are
    /// allowed to parse.
    pub restrict_to_EV_KEY: bool,
    pub namespace: Namespace,
}

impl<'a> KeyParser<'a> {
    /// Returns the KeyParser that is the most commonly used one for filtering events,
    /// e.g. the first key in `--map key:a key:b` but not the second key.
    pub fn default_filter() -> KeyParser<'static> {
        KeyParser {
            default_value: "",
            allow_ranges: true,
            allow_transitions: true,
            allow_types: true,
            restrict_to_EV_KEY: false,
            namespace: Namespace::User,
        }
    }

    /// Returns the KeyParser that is the most commonly used one for masking events,
    /// e.g. the second key in `--map key:a key:b` but not the first key.
    pub fn default_mask() -> KeyParser<'static> {
        KeyParser {
            default_value: "",
            allow_ranges: false,
            allow_transitions: false,
            allow_types: false,
            restrict_to_EV_KEY: false,
            namespace: Namespace::User,
        }
    }

    pub fn with_namespace(&mut self, namespace: Namespace) -> &mut Self {
        self.namespace = namespace;
        self
    }

    pub fn parse(&self, key_str: &str) -> Result<Key, ArgumentError> {
        interpret_key_with_domain(key_str, self)
            .with_context(format!("While parsing the key \"{}\":", key_str))
    }

    pub fn parse_all(&self, key_strs: &[String]) -> Result<Vec<Key>, ArgumentError> {
        key_strs.iter().map(
            |key| self.parse(key)
        ).collect()
    }
}


/// Tells you whether the string looks like a key and should be interpreted as such if it is
/// passed to an argument. Intentionally does not guarantee that it will actually parse as key,
/// so sensible error messages can be given when the user enters a somewhat incorrect key and
/// be told why it isn't a key. If this function worked perfectly, such keys might get seen as
/// flags and the user might end up with useless error messages such as "the --map argument
/// doesn't take a key:lctrl flag".
pub fn resembles_key(key_str: &str) -> bool {
    // Make sure we don't confuse a path for a key.
    if key_str.starts_with('/') {
        false
    } else {
        // Check if it is an actual key.
        KeyParser {
            default_value: "",
            allow_ranges: true,
            allow_transitions: true,
            allow_types: true,
            restrict_to_EV_KEY: false,
            namespace: Namespace::User,
        }.parse(key_str).is_ok()
        // Otherwise, check if it contains some of the key-like characters.
        // No flag or clause name should contain a : or @ to make sure they're not mistaken for keys.
        || utils::split_once(key_str, "=").0.contains(':')
        || utils::split_once(key_str, "=").0.contains('@')
    }
}

/// Interprets a key that optionally has a domain attached, like "key:a@keyboard".
/// The default value is what shall be taken for the range of values if none is specified.
fn interpret_key_with_domain(key_str: &str, parser: &KeyParser) -> Result<Key, ArgumentError> {
    let (event_str, domain_str_opt) = utils::split_once(key_str, "@");
    let mut key = interpret_key(event_str, parser)?;
    
	if let Some(domain_str) = domain_str_opt {
        let domain = domain::resolve(domain_str)?;
        key.properties.push(KeyProperty::Domain(domain));
    }

    Ok(key)
}

fn interpret_key(key_str: &str, parser: &KeyParser) -> Result<Key, ArgumentError> {
    let mut key = Key::new();
    key.add_property(KeyProperty::Namespace(parser.namespace));
	if key_str == "" {
        if ! parser.restrict_to_EV_KEY {
            return Ok(key)
        } else {
            return Err(ArgumentError::new(
                "This argument can only apply to events with type EV_KEY (i.e. key:something and btn:something), and therefore does not accept the empty string as filter."
            .to_owned()));
        }
    }
	
	let mut parts = key_str.split(':');
	
	// Interpret the event type.
    let event_type_name = parts.next().unwrap();
    let event_type = ecodes::event_type(event_type_name).ok_or_else(||
        ArgumentError::new(format!(
            "Unknown event type \"{}\".", event_type_name
        ))
    )?;
    if event_type.is_syn() {
        return Err(ArgumentError::new("Cannot use event type \"syn\": it is impossible to manipulate synchronisation events because synchronisation is automatically taken care of by evsieve."));
    }

    if parser.restrict_to_EV_KEY && event_type != EventType::KEY {
        return Err(ArgumentError::new(format!(
            "This argument can only apply to events with type EV_KEY (i.e. key:something and btn:something), and therefore cannot filter to events with type \"{}\".", event_type_name
        )));
    }

    // Extract the event code, or return a key that matches on type only.
    match parts.next() {
        Some(event_code_name) => {
            let event_code = ecodes::event_code(event_type_name, event_code_name).ok_or_else(||
                ArgumentError::new(format!(
                    "Unknown event code \"{}\".", event_code_name
                ))
            )?;
            key.add_property(KeyProperty::Code(event_code));

            // ISSUE: ABS_MT support
            if ecodes::is_abs_mt(event_code) {
                utils::warn_once("Warning: it seems you're trying to manipulate ABS_MT events. Keep in mind that evsieve's support for ABS_MT is considered unstable. Evsieve's behaviour with respect to ABS_MT events is subject to change in the future.");
            }
        }
        // If no event code is available, then either throw an error or return a key that matches only on
        // the virtual type depending on whether parser.allow_types is set.
        None => {
            if ! parser.allow_types {
                return Err(ArgumentError::new(format!("No event code provided for the key \"{}\".", key_str)));
            }
            // TODO: consider refactoring.
            let virtual_type = match event_type_name {
                "key" => VirtualEventType::Key,
                "btn" => VirtualEventType::Button,
                _ => VirtualEventType::Other(event_type),
            };
            key.add_property(KeyProperty::VirtualType(virtual_type));
        }
    };
    
    let event_value_str = match parts.next() {
        Some(value) => value,
        None => match parser.default_value {
            "" => return Ok(key),
            _ => &parser.default_value,
        },
    };

    // Determine what the previous event value (if any) is, and the current event value.
    let (val_1, val_2) = utils::split_once(event_value_str, "..");
    let (previous_value_str_opt, current_value_str) = match val_2 {
        Some(val) => (Some(val_1), val),
        None => (None, val_1),
    };

    let current_value = interpret_event_value(current_value_str, parser)?;
    key.add_property(KeyProperty::Value(current_value));

    if let Some(previous_value_str) = previous_value_str_opt {
        if ! parser.allow_transitions {
            return Err(ArgumentError::new(
                "No transitions are allowed for keys in this position."
            ));
        }

        let previous_value = interpret_event_value(previous_value_str, parser)?;
        key.add_property(KeyProperty::PreviousValue(previous_value));
    }
    
    Ok(key)
}

/// Interprets a string like "1" or "0~1" or "5~" or "".
fn interpret_event_value(value_str: &str, parser: &KeyParser) -> Result<Range, ArgumentError> {
    if ! parser.allow_ranges && value_str.contains('~') {
        return Err(ArgumentError::new(format!("No ranges are allowed in the value \"{}\".", value_str)));
    }
    
    let (min_value_str, max_value_str_opt) = utils::split_once(value_str, "~");
    let max_value_str = max_value_str_opt.unwrap_or(min_value_str);

    let min = parse_int_or_wildcard(min_value_str)?;
    let max = parse_int_or_wildcard(max_value_str)?;

    if let (Some(min_value), Some(max_value)) = (min, max) {
        if min_value > max_value {
            return Err(ArgumentError::new(format!(
                "The upper bound of a value range may not be smaller than its lower bound. Did you intend to use the range {}~{} instead?", max_value, min_value
            )));
        }
    }

    Ok(Range::new(min, max))
}

/// Returns None for "", an integer for integer strings, and otherwise gives an error.
fn parse_int_or_wildcard(value_str: &str) -> Result<Option<i32>, ArgumentError> {
    if value_str == "" {
        Ok(None)
    } else {
        let value: i32 = value_str.parse().map_err(|err| ArgumentError::new(
            format!("Cannot interpret {} as an integer: {}.", value_str, err)
        ))?;
        Ok(Some(value))
    }
}
