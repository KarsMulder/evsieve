// SPDX-License-Identifier: GPL-2.0-or-later

use crate::event::{EventType, EventCode};
use crate::bindings::libevdev;
use crate::utils::{split_once, parse_cstr};
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
                
                unsafe { result.insert(name, EventType::new(ev_type as u16)) };
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
            let code_max = event_type_get_max(ev_type);
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
                    let event_code = unsafe { EventCode::new(ev_type, code as u16) };

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

    /// For each _valid_ event code (EV_KEY, code) holds: the name of this code starts with
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

pub fn event_type_get_max(ev_type: EventType) -> u16 {
    let result = unsafe { libevdev::libevdev_event_type_get_max(ev_type.into()) };
    result.try_into().unwrap_or(u16::MAX)
}

pub fn event_name(code: EventCode) -> String {
    match EVENT_NAMES.get(&code) {
        Some(name) => name.to_owned(),
        None => format!("{}:{}", u16::from(code.ev_type()), code.code()),
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

pub fn event_type(name: &str) -> Option<EventType> {
    EVENT_TYPES.get(name).cloned()
}

pub fn event_code(type_name: &str, code_name: &str) -> Option<EventCode> {
    EVENT_CODES.get(&(type_name.to_string(), code_name.to_string())).cloned()
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

#[test]
fn unittest() {
    // Since the is_abs_mt function depends on the user-facing representation we use for events,
    // this test makes sure it doesn't accidentally break if we change out naming scheme.
    unsafe {
        assert!(is_abs_mt(EventCode::new(EventType::ABS, 0x35)));
        assert!(!is_abs_mt(EventCode::new(EventType::ABS, 0x01)));
        assert!(!is_abs_mt(EventCode::new(EventType::KEY, 0x35)));
    }
}
