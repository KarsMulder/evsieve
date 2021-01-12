// SPDX-License-Identifier: GPL-2.0-or-later

use crate::event::{EventType, EventCode, EventId};
use crate::bindings::libevdev;
use crate::utils::split_once;
use std::collections::HashMap;
use std::ffi::CStr;
use std::os::raw::c_char;

unsafe fn parse_cstr(raw_ptr: *const c_char) -> Option<String> {
    if raw_ptr == std::ptr::null() {
        return None;
    }
    let raw_cstr: &CStr = CStr::from_ptr(raw_ptr);
    raw_cstr.to_str().ok().map(str::to_string)
}

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
                
                result.insert(name, ev_type as EventType);
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
            let code_max = unsafe { libevdev::libevdev_event_type_get_max(ev_type as u32) };
            for code in 0 ..= code_max {
                if let Some(raw_code_name) = unsafe { parse_cstr(libevdev::libevdev_event_code_get_name(ev_type as u32, code as u32)) } {
                    let (code_type_name, code_name_opt) = split_once(&raw_code_name, "_");

                    // This test helps us not confuse KEY_* with BTN_* events.
                    if &code_type_name.to_lowercase() != ev_type_name {
                        continue;
                    }

                    let code_name = match code_name_opt {
                        Some(name_uppercase) => name_uppercase.to_lowercase(),
                        None => continue,
                    };
                    result.insert((ev_type_name.to_string(), code_name), code as EventCode);
                }
            }
        }

        result
    };

    /// A list of all tuples (event_type, event_code) supported.
    pub static ref EVENT_IDS: Vec<EventId> = {
        EVENT_CODES.iter().filter_map(|((type_str, _code_str), code_val)| {
            Some((*EVENT_TYPES.get(type_str)?, *code_val))
        }).collect()
    };

    pub static ref EVENT_NAMES: HashMap<EventId, String> = {
        let mut result = HashMap::new();
        for ((type_name, code_name), code) in EVENT_CODES.iter() {
            let ev_type = EVENT_TYPES.get(type_name).expect("Invalid ecode data compiled.");
            let name = format!("{}:{}", type_name, code_name);
            result.insert((*ev_type, *code), name);
        }
        result
    };
}

pub fn event_name(ev_type: EventType, code: EventCode) -> String {
    match EVENT_NAMES.get(&(ev_type, code)) {
        Some(name) => name.to_owned(),
        None => format!("{}:{}", ev_type, code),
    }
}

// Returns whether this event is an multitouch event.
pub fn is_abs_mt(ev_type: EventType, code: EventCode) -> bool {
    ev_type == EV_ABS && event_name(ev_type, code).starts_with("abs:mt_")
}

pub fn event_type(name: &str) -> Option<EventType> {
    EVENT_TYPES.get(name).cloned()
}

pub fn event_code(type_name: &str, code_name: &str) -> Option<EventType> {
    EVENT_CODES.get(&(type_name.to_string(), code_name.to_string())).cloned()
}

pub const EV_ABS: EventType = libevdev::EV_ABS as u16;
pub const EV_SYN: EventType = libevdev::EV_SYN as u16;
pub const EV_REP: EventType = libevdev::EV_REP as u16;
pub const EV_KEY: EventType = libevdev::EV_KEY as u16;

pub const REP_DELAY: EventType = libevdev::REP_DELAY as u16;
pub const REP_PERIOD: EventType = libevdev::REP_PERIOD as u16;

#[test]
fn unittest() {
    // Since the is_abs_mt function depends on the user-facing representation we use for events,
    // this test makes sure it doesn't accidentally break if we change out naming scheme.
    assert!(is_abs_mt(EV_ABS, 0x35));
    assert!(!is_abs_mt(EV_ABS, 0x01));
    assert!(!is_abs_mt(EV_KEY, 0x35));
}
