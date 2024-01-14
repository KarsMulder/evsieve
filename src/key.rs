// SPDX-License-Identifier: GPL-2.0-or-later

use crate::affine::AffineFactor;
use crate::domain;
use crate::domain::Domain;
use crate::event::{Event, EventType, EventCode, Channel, Namespace, VirtualEventType};
use crate::utils;
use crate::error::ArgumentError;
use crate::capability::{Capability, CapMatch};
use crate::range::Range;
use crate::ecodes;
use crate::error::Context;

#[derive(Clone, Debug)]
pub struct Key {
    /// Upholds invariant: at most one copy of each KeyProperty variant may be in this vector.
    /// Putting multiple copies in it is a logical error; some functions may not function
    /// correctly if this invariant is broken.
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

    /// Returns true if this Key might match any Event with a given channel.
    pub fn matches_channel(&self, channel: Channel) -> bool {
        self.properties.iter().all(|prop| prop.matches_channel(channel))
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

    /// Makes this key require a certain particular value.
    pub fn set_value(&mut self, value: Range) {
        self.pop_value();
        self.properties.push(KeyProperty::Value(value));
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
                KeyProperty::Type(ev_type) => return Some(*ev_type),
                KeyProperty::VirtualType(v_type) => return Some(v_type.ev_type()),
                KeyProperty::Domain(_)
                | KeyProperty::Namespace(_)
                | KeyProperty::Value(_)
                | KeyProperty::PreviousValue(_)
                | KeyProperty::AffineFactor(_)
                => (),
            }
        }
        None
    }

    /// Returns Some(EventType) if this Key will only ever accept events of a certain code.
    /// If it may accept different types of events, returns None.
    pub fn requires_event_code(&self) -> Option<EventCode> {
        for property in &self.properties {
            match property {
                KeyProperty::Code(code) => return Some(*code),
                KeyProperty::Type(_)
                | KeyProperty::VirtualType(_)
                | KeyProperty::Domain(_)
                | KeyProperty::Namespace(_)
                | KeyProperty::Value(_)
                | KeyProperty::PreviousValue(_)
                | KeyProperty::AffineFactor(_)
                => (),
            }
        }
        None
    }

    /// Removes the value/range requirement from this key and returns it separately if it existed.
    pub fn split_value(mut self) -> (Key, Option<Range>) {
        let mut range_requirement = None;
        self.properties.retain(|property|
            match property {
                KeyProperty::Value(range) => {
                    range_requirement = Some(*range);
                    false
                },
                KeyProperty::Type(_)
                | KeyProperty::Code(_)
                | KeyProperty::VirtualType(_)
                | KeyProperty::Domain(_)
                | KeyProperty::Namespace(_)
                | KeyProperty::PreviousValue(_)
                | KeyProperty::AffineFactor(_)
                => true,
            }
        );

        (self, range_requirement)
    }

    /// Returns true if some event may match both key_1 and key_2.
    pub fn intersects_with(&self, other: &Key) -> bool {
        // Tests interaction between (Type, VirtualType) and (Type, Code).
        if let (Some(ev_type_1), Some(ev_type_2)) = (self.requires_event_type(), other.requires_event_type()) {
            if ev_type_1 != ev_type_2 {
                return false;
            }
        }

        for prop_1 in &self.properties {
            for prop_2 in &other.properties {
                let these_properties_may_intersect = match (prop_1, prop_2) {
                    (KeyProperty::Code(left), KeyProperty::Code(right))
                        => left == right,
                    (KeyProperty::Domain(left), KeyProperty::Domain(right))
                        => left == right,
                    (KeyProperty::Namespace(left), KeyProperty::Namespace(right))
                        => left == right,

                    (KeyProperty::VirtualType(left), KeyProperty::VirtualType(right))
                        => left == right,
                    (KeyProperty::VirtualType(v_type), KeyProperty::Code(code))
                    | (KeyProperty::Code(code), KeyProperty::VirtualType(v_type))
                        => *v_type == code.virtual_ev_type(),

                    (KeyProperty::Value(left), KeyProperty::Value(right))
                    | (KeyProperty::PreviousValue(left), KeyProperty::PreviousValue(right))
                        => left.intersects_with(right),
                    
                    (KeyProperty::Code(_), _)
                    | (KeyProperty::Domain(_), _)
                    | (KeyProperty::Namespace(_), _)
                    | (KeyProperty::Type(_), _)
                    | (KeyProperty::VirtualType(_), _)
                    | (KeyProperty::Value(_), _)
                    | (KeyProperty::PreviousValue(_), _)
                    | (KeyProperty::AffineFactor(_), _)
                        => true,
                };
                if ! these_properties_may_intersect {
                    return false;
                }
            }
        }

        true
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
    Type(EventType),
    /// Only valid for filter keys.
    VirtualType(VirtualEventType),
    /// Applies an affine transformation on the input event.
    /// Only valid for mask keys.
    AffineFactor(AffineFactor),
}

impl KeyProperty {
    /// Checkes whether an event matches this KeyProperty.
    pub fn matches(&self, event: &Event) -> bool {
        match *self {
            KeyProperty::Code(value) => event.code == value,
            KeyProperty::Domain(value) => event.domain == value,
            KeyProperty::Type(value) => event.code.ev_type() == value,
            KeyProperty::VirtualType(value) => event.code.virtual_ev_type() == value,
            KeyProperty::Namespace(value) => event.namespace == value,
            KeyProperty::Value(range) => range.contains(event.value),
            KeyProperty::PreviousValue(range) => range.contains(event.previous_value),
            KeyProperty::AffineFactor(_) => {
                // Similarly to `KeyProperty::merge`, benchmarks show that the mere threat of panicking
                // during this function can significantly reduce performance, therefore this assertion
                // is only made during debug builds.
                if cfg!(debug_assertions) {
                    panic!("Cannot filter events based on relative values. Panicked during event mapping.");
                }
                false
            },
        }
    }

    /// Checks whether this Keyproperty might match any event with a given channel.
    pub fn matches_channel(&self, channel: Channel) -> bool {
        let (code, domain) = channel;
        match *self {
            KeyProperty::Code(value) => code == value,
            KeyProperty::Domain(value) => domain == value,
            KeyProperty::Type(value) => value == code.ev_type(),
            KeyProperty::VirtualType(value) => value.ev_type() == code.ev_type(),
            KeyProperty::Namespace(_)
            | KeyProperty::Value(_)
            | KeyProperty::PreviousValue(_)
            | KeyProperty::AffineFactor(_)
                => true,
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
            KeyProperty::AffineFactor(factor) => {
                event = factor.merge(event);
            },
            KeyProperty::Type(_) | KeyProperty::VirtualType(_) => {
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
            KeyProperty::Type(value) => (cap.code.ev_type() == value).into(),
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
            KeyProperty::AffineFactor(_) => {
                panic!("Internal invariant violated: cannot filter events based on relative values.");
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
            KeyProperty::AffineFactor(factor) => cap = factor.merge_cap(cap),
            KeyProperty::Type(_) | KeyProperty::VirtualType(_) => {
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
    /// Is Some, then it only allows keys that require this type or have no type/code requirements.
    /// Forbids keys that that require a type/code outside this range.
    pub type_whitelist: Option<Vec<EventType>>,

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
            type_whitelist: None,
            namespace: Namespace::User,
        }
    }

    /// Returns a KeyParser that functions the same as self, except that it will only
    /// parse keys that would be deemed to be valid keys according to `other` as well.
    /// 
    /// Does not guarantee that `self` and `other` would parse keys to the same key.
    pub fn and_filter(self, other: KeyParser) -> Self {
        let merged_whitelist = match (self.type_whitelist, other.type_whitelist) {
            (None, None) => None,
            (None, whitelist) | (whitelist, None) => whitelist,
            (Some(list1), Some(list2)) => {
                let mut joined_list: Vec<_> = list1.into_iter().chain(list2).collect();
                joined_list.sort();
                joined_list.dedup();
                Some(joined_list)
            }
        };
        KeyParser {
            default_value: self.default_value,
            allow_values: self.allow_values && other.allow_values,
            allow_transitions: self.allow_transitions && other.allow_transitions,
            allow_ranges: self.allow_ranges && other.allow_ranges,
            allow_types: self.allow_types && other.allow_types,
            allow_relative_values: self.allow_relative_values && other.allow_relative_values,
            type_whitelist: merged_whitelist,
            namespace: self.namespace,
        }
    }

    /// Returns the KeyParser that is the most commonly used one for masking events,
    /// e.g. the second key in `--map key:a key:b` but not the first key.
    pub fn default_mask() -> KeyParser<'static> {
        KeyParser {
            default_value: "",
            allow_values: true,
            allow_ranges: true,
            allow_transitions: false,
            allow_types: false,
            allow_relative_values: true,
            type_whitelist: None,
            namespace: Namespace::User,
        }
    }

    /// Returns a Keyparser that only interprets "pure" keys, i.e. keys for which
    /// `key.matches(event) == key.matches_channel(event.channel)` is true for all events.
    pub fn pure() -> KeyParser<'static> {
        KeyParser {
            default_value: "",
            allow_values: false,
            allow_ranges: false,
            allow_transitions: false,
            allow_types: true,
            allow_relative_values: false,
            type_whitelist: None,
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
        let before_equals_part = utils::split_once(key_str, "=").0;

        // Check if it is an actual key.
        KeyParser {
            default_value: "",
            allow_values: true,
            allow_ranges: true,
            allow_transitions: true,
            allow_types: true,
            allow_relative_values: true,
            type_whitelist: None,
            namespace: Namespace::User,
        }.parse(key_str).is_ok()
        // Otherwise, check if it contains some of the key-like characters.
        // No flag or clause name should contain a : or @ to make sure they're not mistaken for keys.
        || before_equals_part.contains(':')
        || before_equals_part.contains('@')
        || before_equals_part.contains('%')
    }
}

/// Interprets a key that optionally has a domain attached, like "key:a@keyboard".
fn interpret_key_with_domain(key_str: &str, parser: &KeyParser) -> Result<Key, ArgumentError> {
    let parts = key_str_to_parts(key_str)?;
    let mut key = interpret_key(parts, parser)?;

    if let Some(domain_str) = parts.domain {
        let domain = domain::resolve(domain_str)?;
        key.properties.push(KeyProperty::Domain(domain));
    }

    Ok(key)
}

#[derive(Clone, Copy)]
struct KeyParts<'a> {
    /// The full string of which these parts were lexed.
    key_str: &'a str,

    // The following three fields represent respectively the "key", "a" and "1" parts of a string
    // like "key:a:1". They will never equal Some(""); in those cases they should be turned into
    // None instead.
    ev_type: Option<&'a str>,
    code: Option<&'a str>,
    value: Option<&'a str>,

    domain: Option<&'a str>,
}

fn key_str_to_parts(key_str: &str) -> Result<KeyParts, ArgumentError> {
    let (key_str, domain) = utils::split_once(key_str, "@");

    let mut parts_iter = key_str.split(':').peekable();

    // Make sure that we never store the empty string in the type, code, or value options.
    fn treat_empty_as_none(option: Option<&str>) -> Option<&str> {
        option.filter(|content| !content.is_empty())
    }
    let ev_type = treat_empty_as_none(parts_iter.next());
    let code = treat_empty_as_none(parts_iter.next());
    let value = treat_empty_as_none(parts_iter.next());

    // This forbids keys like "key:", "key:a:" or "key::".
    if key_str.ends_with(':') {
        return Err(ArgumentError::new(
            format!(
                "A key must not end on a colon (\":\"). Please try \"{}\" instead.",
                key_str.trim_end_matches(':')
            )
        ));
    }

    // Make sure there is nothing after the last colon, such as in key:a:1:2.
    // TODO: TEST THIS
    if parts_iter.peek().is_some() {
        let superfluous_part = parts_iter.collect::<Vec<_>>().join(":");
        return Err(ArgumentError::new(format!(
            "Too many colons encountered in the key \"{}\". There is no way to interpret the \":{}\" part.", key_str, superfluous_part
        )));
    }

    Ok(KeyParts {
        key_str,
        ev_type,
        code,
        value,
        domain,
    })
}

fn interpret_key(parts: KeyParts, parser: &KeyParser) -> Result<Key, ArgumentError> {
    let mut key = Key::new();
    key.add_property(KeyProperty::Namespace(parser.namespace));

    if parts.code.is_some() && parts.ev_type.is_none() {
        // TODO: LOW-PRIORITY: Consider allowing this instead of throwing an error.
        return Err(ArgumentError::new("Cannot specify event code or value without specifying event type."));
    }

    // Interpret the event type.
    if let Some(event_type_name) = parts.ev_type {
        let event_type = ecodes::event_type(event_type_name)?;

        if event_type.is_syn() {
            return Err(ArgumentError::new("Cannot use event type \"syn\": it is impossible to manipulate synchronisation events because synchronisation is automatically taken care of by evsieve."));
        }
        if let Some(whitelist) = &parser.type_whitelist {
            if ! whitelist.contains(&event_type) {
                // Return an error message depending on what the whitelist was.
                if whitelist == &[EventType::KEY] {
                    return Err(ArgumentError::new(
                        "Only events of type EV_KEY (i.e. \"key:something\" or \"btn:something\") can be specified in this position."
                    ));
                } else if let Some(example_type) = whitelist.first() {
                    let allowed_keys = whitelist.iter().map(|ev_type| ecodes::type_name(*ev_type))
                        .collect::<Vec<_>>().join(", ");
                    let example_name = ecodes::type_name(*example_type);
                    let plural = match whitelist.len() {
                        1 => "",
                        _ => "s",
                    };

                    return Err(ArgumentError::new(
                        format!("Only events of type{plural} {allowed_keys} (i.e. \"{example_name}:something\") can be specified in this position.")
                    ));
                } else {
                    return Err(ArgumentError::new(
                        "No specific event type can can be specified in this position."
                    ));
                }
            }
        }

        // Extract the event code, or set a property that matches on type only.
        match parts.code {
            // If no event code is available, then either throw an error or return a key that matches only on
            // the virtual type depending on whether parser.allow_types is set.
            //
            // The Some("") case should never happen, but we add this match for defensive programming.
            None | Some("") => {
                if ! parser.allow_types {
                    return Err(ArgumentError::new(format!("No event code provided for the key \"{}\".", parts.key_str)));
                }

                let property = match event_type_name {
                    VirtualEventType::KEY => KeyProperty::VirtualType(VirtualEventType::Key),
                    VirtualEventType::BUTTON => KeyProperty::VirtualType(VirtualEventType::Button),
                    _ => KeyProperty::Type(event_type),
                };
                key.add_property(property);
            },
            Some(event_code_name) => {
                let event_code = ecodes::event_code(event_type_name, event_code_name)?;
                key.add_property(KeyProperty::Code(event_code));

                // ISSUE: ABS_MT support
                if ecodes::is_abs_mt(event_code) {
                    utils::warn_once("Warning: it seems you're trying to manipulate ABS_MT events. Keep in mind that evsieve's support for ABS_MT is considered unstable. Evsieve's behaviour with respect to ABS_MT events is subject to change in the future.");
                }
            }
        };
    } 

    let event_value_str = match parts.value {
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
            _ => parser.default_value,
        },
    };

    // Check if it is a relative value.
    match interpret_relative_value(event_value_str) {
        AffineParseResult::IsAffine(property) => {
            if parser.allow_relative_values {
                key.add_property(property);
                return Ok(key);
            } else {
                return Err(ArgumentError::new(format!(
                    "It is not possible to specify relative values for the key {}.", parts.key_str,
                )))
            }
        },
        AffineParseResult::IsConstant(property) => {
            if parser.allow_relative_values {
                key.add_property(property);
                return Ok(key);
            } else {
                // Do nothing.
                //
                // We do not want to accept it yet because that could lead to cases like
                // ::4+0x getting accepted where it shouldn't, but we shouldn't throw an
                // error either because that could cause errors on ::4 getting interpreted
                // as an affine factor;
            }
        },
        AffineParseResult::Unparsable => (),
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

enum AffineParseResult {
    // This value is an actual affine factor.
    IsAffine(KeyProperty),
    // This value turns out to be constant.
    IsConstant(KeyProperty),
    // This value is neither affine nor a constant, but it may be something else, like a range.
    Unparsable,
}

/// Parses a value like "0.1d" or "-x".
fn interpret_relative_value(value_str: &str) -> AffineParseResult {
    let factor = match crate::affine::parse_affine_factor(value_str) {
        Ok(factor) => factor,
        Err(_) => return AffineParseResult::Unparsable,
    };
    if let Some(value) = factor.as_constant() {
        if value.trunc() != value {
            return AffineParseResult::Unparsable;
        };
        let value_as_i32: i32 = value.trunc() as i32;
        AffineParseResult::IsConstant(KeyProperty::Value(Range::new(value_as_i32, value_as_i32)))
    } else {
        AffineParseResult::IsAffine(KeyProperty::AffineFactor(factor))
    }
}

#[test]
fn unittest_intersection() {
    let parser = KeyParser::default_filter();
    let expected_to_intersect = [
        ("key", "key"),
        ("key:a", "key:a"),
        ("key", "key:a"),
        ("key:a", "key"),
        ("key:a:1..2@foo", "key:a:1..2@foo"),
        ("key:a:1..2@foo", "@foo"),
        ("", ""),
        ("", "key:a:1..2@foo"),
        ("%1", "%1"),
        ("%1", "key"),
        ("%1", "key:left"),
        ("%1", "btn:left"),
        ("%1", ""),
    ];
    let expected_not_to_intersect = [
        ("key:a", "key:b"),
        ("key:a", "btn"),
        ("btn", "key:a"),
        ("key", "btn"),
        ("abs", "rel"),
        ("%1", "%2"),
        ("key:a@foo", "key:a@bar"),
        ("key:a@foo", "@bar"),
        ("key:a:1", "key:a:2"),
        ("key:a:1..2", "key:a:0..2"),
        ("%1", "abs:x"),
    ];

    for (key_1, key_2) in expected_to_intersect {
        assert!(parser.parse(key_1).unwrap().intersects_with(&parser.parse(key_2).unwrap()));
        assert!(parser.parse(key_2).unwrap().intersects_with(&parser.parse(key_1).unwrap()));
    }
    for (key_1, key_2) in expected_not_to_intersect {
        assert!(! parser.parse(key_1).unwrap().intersects_with(&parser.parse(key_2).unwrap()));
        assert!(! parser.parse(key_2).unwrap().intersects_with(&parser.parse(key_1).unwrap()));
    }
}

#[test]
fn unittest_requires_range() {
    let parser = KeyParser::default_filter();
    assert!(parser.parse("abs:x").unwrap().split_value().1.is_none());
    assert!(parser.parse("abs:x:~").unwrap().split_value().1 == Some(Range::new(None, None)));
    assert!(parser.parse("abs:x:1").unwrap().split_value().1 == Some(Range::new(1, 1)));
    assert!(parser.parse("abs:x:1~1").unwrap().split_value().1 == Some(Range::new(1, 1)));
}
