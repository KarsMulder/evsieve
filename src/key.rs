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

    /// Returns Some(EventType) if this Key will only ever accept events of a certain type.
    /// If it may accept different types of events, returns None.
    pub fn requires_event_type(&self) -> Option<EventType> {
        for property in &self.properties {
            match property {
                KeyProperty::Code(code) => return Some(code.ev_type()),
                KeyProperty::VirtualType(v_type) => return Some(v_type.ev_type()),
                KeyProperty::Domain(_)
                | KeyProperty::Namespace(_)
                | KeyProperty::Value(_)
                | KeyProperty::PreviousValue(_)
                | KeyProperty::DeltaFactor(_)
                => (),
            }
        }
        None
    }
}

#[derive(Clone, Copy, Debug)]
enum KeyProperty {
    Code(EventCode),
    Domain(Domain),
    Namespace(Namespace),
    Value(Range),
    PreviousValue(Range),
    /// Only valid for filter keys.
    VirtualType(VirtualEventType),
    /// Designates that the value of the output event should be a given factor of
    /// (event_in.value - event_in.previous_value). Only valid for mask keys.
    DeltaFactor(f64),
    // TODO: consider adding a ValueFactor as well.
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
            KeyProperty::DeltaFactor(_) => {
                if cfg!(debug_assertions) {
                    panic!("Cannot filter events based on delta values. Panicked during event mapping.");
                }
                false
            },
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
            KeyProperty::DeltaFactor(factor) => {
                // Putting the `floor()` calls at these specific places makes this algorithm more
                // resistant to rounding errors than doing it the straightforward way.
                event.value = (
                    (event.value as f64 * factor).floor()
                    - (event.previous_value as f64 * factor).floor()
                ) as i32;
            }
            KeyProperty::VirtualType(_) => {
                if cfg!(debug_assertions) {
                    panic!("Cannot change the event type of an event. Panicked during event mapping.");
                } else {
                    // Do nothing.
                    //
                    // I would like to print a warning here, but benchmarks show that makes evsieve's
                    // performance 33% slower, just for a situation that should both never happen and
                    // if it did happen, would most likely be caught during capability propagation.
                    //
                    // utils::warn_once("Internal error: cannot change the event type of an event. If you see this message, this is a bug.");
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
            KeyProperty::DeltaFactor(_) => {
                panic!("Internal invariant violated: cannot filter events based on delta values.");
            },
        }
    }

    pub fn merge_cap(&self, mut cap: Capability) -> Capability {
        match *self {
            KeyProperty::Code(value) => cap.code = value,
            KeyProperty::Domain(value) => cap.domain = value,
            KeyProperty::Namespace(value) => cap.namespace = value,
            KeyProperty::Value(range) => cap.value_range = range.bound_range(&cap.value_range),
            KeyProperty::PreviousValue(_range) => {},
            KeyProperty::DeltaFactor(factor) => {
                // This floor rounding matches the algorithm used for event propagation.
                let bound_1 = cap.value_range.max.mul_f64_floor(factor);
                let bound_2 = cap.value_range.min.mul_f64_floor(factor);
                let max = std::cmp::max(bound_1, bound_2);
                let min = std::cmp::min(bound_1, bound_2);
                cap.value_range = Range { max, min };
            },
            KeyProperty::VirtualType(_) => {
                if cfg!(debug_assertions) {
                    panic!("Cannot change the event type of an event. Panicked during capability propagation.");
                } else {
                    utils::warn_once("Internal error: cannot change the event type of an event. If you see this message, this is a bug.");
                }
            },
        };
        cap
    }
}

/// Represents the options for how a key can be parsed in different contexts.
pub struct KeyParser<'a> {
    pub default_value: &'a str,
    /// Whether event values like the :1 in "key:a:1" are allowed.
    pub allow_values: bool,
    pub allow_transitions: bool,
    pub allow_ranges: bool,
    /// Whether keys with only a type like "key", "btn", "abs", and such without an event code, are allowed.
    /// Only ever set this to true for filter keys.
    pub allow_types: bool,
    /// Whether keys with an event value that depends on which event is getting masked, are allowed.
    /// Only ever set this to true for mask keys.
    pub allow_relative_values: bool,

    pub namespace: Namespace,
}

impl<'a> KeyParser<'a> {
    /// Returns the KeyParser that is the most commonly used one for filtering events,
    /// e.g. the first key in `--map key:a key:b` but not the second key.
    pub fn default_filter() -> KeyParser<'static> {
        KeyParser {
            default_value: "",
            allow_values: true,
            allow_ranges: true,
            allow_transitions: true,
            allow_types: true,
            allow_relative_values: false,
            namespace: Namespace::User,
        }
    }

    /// Returns the KeyParser that is the most commonly used one for masking events,
    /// e.g. the second key in `--map key:a key:b` but not the first key.
    pub fn default_mask() -> KeyParser<'static> {
        KeyParser {
            default_value: "",
            allow_values: true,
            allow_ranges: false,
            allow_transitions: false,
            allow_types: false,
            allow_relative_values: true,
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
            allow_values: true,
            allow_ranges: true,
            allow_transitions: true,
            allow_types: true,
            allow_relative_values: true,
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
        return Ok(key)
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

            let virtual_type = match event_type_name {
                VirtualEventType::KEY => VirtualEventType::Key,
                VirtualEventType::BUTTON => VirtualEventType::Button,
                _ => VirtualEventType::Other(event_type),
            };
            key.add_property(KeyProperty::VirtualType(virtual_type));
        }
    };
    
    let event_value_str = match parts.next() {
        Some(value) => {
            if parser.allow_values {
                value
            } else {
                return Err(ArgumentError::new(format!(
                    "This argument does not allow you to specify values for its events. Try removing the \":{}\" part.",
                    value
                )))
            }
        },
        None => match parser.default_value {
            "" => return Ok(key),
            _ => &parser.default_value,
        },
    };

    // Check if it is a relative value.
    if let Some(property) = interpret_relative_value(event_value_str)? {
        if parser.allow_relative_values {
            key.add_property(property);
            return Ok(key);
        } else {
            return Err(ArgumentError::new(format!(
                "It is not possible to specify relative values for the key {}.", key_str,
            )))
        }
    }

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

/// Interprets a string like "1" or "0~1" or "5~" or "". Does not handle relative values.
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

/// Parses a value like "0.1d".
///
/// Returns Ok(None) if value_str does not look like a relative value. Returns Err if it does look
/// like a relative value, but its format is unacceptable for some reason.
fn interpret_relative_value(value_str: &str) -> Result<Option<KeyProperty>, ArgumentError> {
    match utils::strip_suffix(value_str, "d") {
        None => Ok(None),
        Some(factor_str) => {
            let factor = match utils::parse_number(factor_str) {
                Some(factor) => factor,
                None => return Err(ArgumentError::new(format!(
                    "Cannot interpret {} as a float.", factor_str
                ))),
            };
            Ok(Some(KeyProperty::DeltaFactor(factor)))
        }
    }
}
