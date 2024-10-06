// SPDX-License-Identifier: GPL-2.0-or-later

use crate::capability::Capability;
use crate::data::hid_usage::UsageNames;
use crate::key::Key;
use crate::event::{Event, EventCode, EventType, EventValue};
use crate::ecodes;
use crate::domain;

pub enum EventPrintMode {
    Detailed,
    Direct,
}

/// Created by --print arguments.
pub struct EventPrinter {
    keys: Vec<Key>,
    mode: EventPrintMode,
}

impl EventPrinter {
    pub fn new(keys: Vec<Key>, mode: EventPrintMode) -> EventPrinter {
        EventPrinter { keys, mode }
    }

    fn apply(&self, event: Event) {
        if self.keys.iter().any(|key| key.matches(&event)) {
            println!("{}", match self.mode {
                EventPrintMode::Direct => print_event_direct(event),
                EventPrintMode::Detailed => print_event_detailed(event),
            });
        }
    }

    pub fn apply_to_all(&self, events: &[Event]) {
        for &event in events {
            self.apply(event);
        }
    }

    pub fn observe_caps(&self, caps: &[Capability]) {
        // If this --print may need to print any event of code msc:scan, then we try to load the
        // scancodes that may be provided by a third-party crate.
        if caps.iter().any(|cap| cap.code == EventCode::MSC_SCAN) {
            crate::data::hid_usage::preload_hid_pages();
        }
    }
}

/// Given the value of a msc:scan event, tries to interpret the value according to the USB HID usage tables.
fn format_hidinfo(value: EventValue) -> Option<String> {
    let pages = crate::data::hid_usage::HID_PAGES.get()?;
    let info = pages.get_usage_from_scancode(value)?;
    if let UsageNames::Known { page_name, usage_name } = info.names {
        Some(format!(" ({}/{})", page_name, usage_name))
    } else {
        None
    }
}

pub fn print_event_detailed(event: Event) -> String {
    let name = ecodes::event_name(event.code);
    let value_str = match event.ev_type() {
        EventType::KEY => match event.value {
            0 => "0 (up)".to_string(),
            1 => "1 (down)".to_string(),
            2 => "2 (repeat)".to_string(),
            _ => format!("{}", event.value),
        },
        EventType::MSC if event.code == EventCode::MSC_SCAN => {
            match format_hidinfo(event.value) {
                Some(info) => format!("{}{}", event.value, info),
                None => format!("{}", event.value),
            }
        },
        _ => format!("{}", event.value),
    };
    let mut result = format!("Event:  type:code = {:<13}  value = {}", name, value_str);

    if let Some(domain_name) = domain::try_reverse_resolve(event.domain) {
        result = format!("{:<80}  domain = {}", result, domain_name);
    }

    result
}

pub fn print_event_direct(event: Event) -> String {
    let name = ecodes::event_name(event.code);
    if let Some(domain_name) = domain::try_reverse_resolve(event.domain) {
        format!("{}:{}@{}", name, event.value, domain_name)
    } else {
        format!("{}:{}", name, event.value)
    }
}