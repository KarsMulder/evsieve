// SPDX-License-Identifier: GPL-2.0-or-later

use crate::error::ArgumentError;
use crate::event::{EventType, EventCode, VirtualEventType};
use crate::bindings::libevdev;
use crate::utils::{split_once, parse_cstr};
use std::borrow::Cow;
use std::collections::HashMap;
use std::convert::TryInto;

lazy_static! {
    pub static ref EVENT_TYPES: HashMap<String, EventType> = {
        let mut result = HashMap::new();
        for ev_type in 0 ..= libevdev::EV_MAX {
            if let Some(raw_name) = unsafe { parse_cstr(libevdev::libevdev_event_type_get_name(ev_type)) } {
                let (ev_prefix, name_opt) = split_once(&raw_name, "_");
                // This should not happen at the time of writing, but we make this check in case some
                // exotic new event type is added in the future.
                if ev_prefix != "EV" {
                    continue;
                }
                let name = match name_opt {
                    Some(name_uppercase) => name_uppercase.to_lowercase(),
                    None => continue,
                };
                
                result.insert(name, EventType::new(ev_type as u16));
            }
        }

        // Special disposition to make keys like btn:left able to map to EV_KEY, BTN_LEFT event
        // though EV_BTN does not exist.
        result.insert(
            "btn".to_string(),
            *result.get("key").expect("Failed to import event type data from libevdev.")
        );
        result
    };

    pub static ref EVENT_CODES: HashMap<(String, String), EventCode> = {
        let mut result = HashMap::new();
        for (ev_type_name, &ev_type) in EVENT_TYPES.iter() {
            let code_max = match event_type_get_max(ev_type) {
                Some(max) => max,
                None => continue,
            };

            for code in 0 ..= code_max {
                if let Some(raw_code_name) = unsafe { parse_cstr(libevdev::libevdev_event_code_get_name(ev_type.into(), code as u32)) } {
                    let (code_type_name, code_name_opt) = split_once(&raw_code_name, "_");

                    // This test helps us not confuse KEY_* with BTN_* events.
                    if &code_type_name.to_lowercase() != ev_type_name {
                        continue;
                    }

                    let code_name = match code_name_opt {
                        Some(name_uppercase) => name_uppercase.to_lowercase(),
                        None => continue,
                    };
                    let event_code = EventCode::new(ev_type, code as u16);

                    result.insert((ev_type_name.to_string(), code_name), event_code);
                }
            }
        }

        result
    };

    pub static ref EVENT_NAMES: HashMap<EventCode, String> = {
        let mut result = HashMap::new();
        for ((type_name, code_name), &code) in EVENT_CODES.iter() {
            let name = format!("{}:{}", type_name, code_name);
            result.insert(code, name);
        }
        result
    };

    /// For each _named_ event code (EV_KEY, code) holds: the name of this code starts with
    /// btn: if and only if it is contained in one of the following ranges.
    ///
    /// This is precomputed for performance reasons: it provides about 30% speedup in scripts
    /// where most maps are type-level on the "key" or "btn" type. It is also lazily computed
    /// so it does not bother users who don't use type-level maps.
    pub static ref BTN_CODE_RANGES: Vec<std::ops::Range<u16>> = {
        // Compute for each valid code whether (EV_KEY, code) is called btn:something or not.
        let mut is_btn_vec: Vec<(u16, bool)> = EVENT_CODES.iter()
            .filter(|(_, code)| code.ev_type().is_key())
            .map(|((typename, _codename), code)| (code.code(), typename == "btn"))
            .collect();
        is_btn_vec.sort_unstable();

        // Detect consecutive ranges of (code, true) in `is_btn_vec`.
        let mut ranges: Vec<std::ops::Range<u16>> = Vec::new();
        let mut range_start: Option<u16> = None;

        for &(code, is_btn) in &is_btn_vec {
            if is_btn {
                if range_start.is_none() {
                    range_start = Some(code);
                }
            } else if let Some(start) = range_start {
                // Remember that ranges are exclusive.
                ranges.push(start .. code);
                range_start = None;
            }
        }

        if let Some(start) = range_start {
            if let Some((last_code, true)) = is_btn_vec.last() {
                ranges.push(start .. last_code + 1)
            }
        }

        ranges
    };
}

pub fn event_type_get_max(ev_type: EventType) -> Option<u16> {
    let result = unsafe { libevdev::libevdev_event_type_get_max(ev_type.into()) };
    result.try_into().ok()
}

pub fn type_name(ev_type: EventType) -> Cow<'static, str> {
    for (name, &type_) in EVENT_TYPES.iter() {
        if ev_type == type_ {
            return Cow::from(name);
        }
    }

    return Cow::from(format!("{}", u16::from(ev_type)));
}

pub fn virtual_type_name(virtual_type: VirtualEventType) -> Cow<'static, str> {
    match virtual_type {
        VirtualEventType::Key => Cow::from(VirtualEventType::KEY),
        VirtualEventType::Button => Cow::from(VirtualEventType::BUTTON),
        VirtualEventType::Other(ev_type) => type_name(ev_type),
    }
}

pub fn event_name(code: EventCode) -> Cow<'static, str> {
    match EVENT_NAMES.get(&code) {
        Some(name) => Cow::from(name),
        None => {
            let type_name = virtual_type_name(code.virtual_ev_type());
            Cow::from(format!("{}:%{}", type_name, code.code()))
        },
    }
}

/// Returns true if this code is of type EV_KEY with a BTN_* code.
pub fn is_button_code(code: EventCode) -> bool {
    code.ev_type().is_key() &&
        BTN_CODE_RANGES.iter().any(|range| range.contains(&code.code()))
}

/// Returns whether this event is an multitouch event.
pub fn is_abs_mt(code: EventCode) -> bool {
    code.ev_type().is_abs() && event_name(code).starts_with("abs:mt_")
}

/// Parses an event type by name like "key" or number like "%1".
pub fn event_type(name: &str) -> Result<EventType, ArgumentError> {
    if let Some(&ev_type) = EVENT_TYPES.get(name) {
        return Ok(ev_type);
    }

    let name_numstr = match name.strip_prefix('%') {
        Some(string) => string,
        None => return Err(ArgumentError::new(format!(
            "Unknown event type \"{}\".", name
        ))),
    };

    let type_u16: u16 = match name_numstr.parse() {
        Ok(code) => code,
        Err(_) => return Err(ArgumentError::new(format!(
            "Cannot interpret \"{}\" as a nonnegative integer.", name_numstr
        ))),
    };

    // TODO: should this (and similar for codes) be a strict inequality?
    if type_u16 <= EV_MAX {
        for &ev_type in EVENT_TYPES.values() {
            if type_u16 == ev_type.into() {
                return Ok(ev_type);
            }
        }

        Err(ArgumentError::new(format!(
            "No event type with numeric value {} exists.",
            type_u16
        )))
    } else {
        Err(ArgumentError::new(format!(
            "Event type {} exceeds the maximum value of {} defined by EV_MAX.",
            type_u16, EV_MAX
        )))
    }
}

/// Parses an event code by name like "key","a" or by name-number pair like "key","%35".
pub fn event_code(type_name: &str, code_name: &str) -> Result<EventCode, ArgumentError> {
    // Check whether the type and code can be interpreted as names.
    if let Some(&code) = EVENT_CODES.get(&(type_name.to_string(), code_name.to_string())) {
        return Ok(code)
    }

    // Check for a (name, number) pair.
    let code_name_numstr = match code_name.strip_prefix('%') {
        Some(string) => string,
        None => {
            // Return a specifically helpful error if the user entered a number as code, e.g.
            // tried btn:300 instead of btn:%300.
            if code_name.parse::<u16>().is_ok() {
                return Err(ArgumentError::new(format!(
                    "Unknown event code \"{}:{}\". (Tip: if you meant to specify an event of type {} and a code of numeric value {}, then you need to add a % prefix like this: \"{}:%{}\")", type_name, code_name, type_name, code_name, type_name, code_name
                )));
            } else {
                return Err(ArgumentError::new(format!(
                    "Unknown event code \"{}:{}\".", type_name, code_name
                )));
            }
        }
    };

    let ev_type = event_type(type_name)?;
    let ev_type_max = match event_type_get_max(ev_type) {
        Some(max) => max,
        None => return Err(ArgumentError::new(format!(
            "No valid event codes exist for event type {}.", type_name,
        ))),
    };
    let code_u16: u16 = match code_name_numstr.parse() {
        Ok(code) => code,
        Err(_) => return Err(ArgumentError::new(format!(
            "Cannot interpret \"{}\" as a nonnegative integer.", code_name_numstr
        ))),
    };

    if code_u16 <= ev_type_max {
        let code = EventCode::new(ev_type, code_u16);
        if ! EVENT_NAMES.contains_key(&code) {
            crate::utils::warn_once(format!(
                "Warning: no event code {}:{} is known to exist. Working with such events may yield unexpected results.", type_name, code_name
            ));
        }

        let virtual_type = code.virtual_ev_type();
        if (type_name == VirtualEventType::KEY && virtual_type != VirtualEventType::Key)
            || (type_name == VirtualEventType::BUTTON && virtual_type != VirtualEventType::Button)
        {
            crate::utils::warn_once(format!(
                "Info: {}:{} shall be interpreted as {}.", type_name, code_name, event_name(code)
            ))
        }
        
        Ok(code)
    } else {
        Err(ArgumentError::new(format!(
            "Event code {} exceeds the maximum value of {} for events of type {}.",
            code_u16, ev_type_max, type_name 
        )))
    }
}

pub const EV_ABS: u16 = libevdev::EV_ABS as u16;
pub const EV_SYN: u16 = libevdev::EV_SYN as u16;
pub const EV_REP: u16 = libevdev::EV_REP as u16;
pub const EV_KEY: u16 = libevdev::EV_KEY as u16;
pub const EV_MSC: u16 = libevdev::EV_MSC as u16;
pub const EV_MAX: u16 = libevdev::EV_MAX as u16;

pub const REP_DELAY: u16 = libevdev::REP_DELAY as u16;
pub const REP_PERIOD: u16 = libevdev::REP_PERIOD as u16;
pub const MSC_SCAN: u16 = libevdev::MSC_SCAN as u16;

/// Returns an iterator over all event types that fall within EV_MAX,
/// whether those types are named or not.
/// TODO: Should EV_MAX itself be checked as well?
pub fn event_types() -> impl Iterator<Item=EventType> {
    (0 ..= EV_MAX).map(EventType::new)
}

/// Returns an iterator over all event codes that fall within their
/// types maximum, whether those types are named or not.
pub fn event_codes_for(ev_type: EventType) -> impl Iterator<Item=EventCode> {
    // I tested: it is possible to write events like key:max to event devices.
    event_type_get_max(ev_type).into_iter().flat_map(move |max|
        (0 ..= max).map(move |code| EventCode::new(ev_type, code))
    )
}

#[test]
fn unittest() {
    // Since the is_abs_mt function depends on the user-facing representation we use for events,
    // this test makes sure it doesn't accidentally break if we change out naming scheme.
    assert!(is_abs_mt(EventCode::new(EventType::ABS, 0x35)));
    assert!(!is_abs_mt(EventCode::new(EventType::ABS, 0x01)));
    assert!(!is_abs_mt(EventCode::new(EventType::KEY, 0x35)));
}
