// SPDX-License-Identifier: GPL-2.0-or-later

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
}

/// Given the value of a msc:scan event, tries to interpret the value according to the USB HID usage tables.
fn format_hidinfo(value: EventValue) -> String {
    let info = crate::data::hid_usage::get_usage_from_scancode(value);

    let usage_names = match info.names {
        UsageNames::Unknown => "".to_owned(),
        UsageNames::PageKnown { page_name } => format!("; {}", page_name),
        UsageNames::Known { page_name, usage_name } => format!("; {} / {}", page_name, usage_name),
    };

    format!("(HID usage page 0x{:02x} id 0x{:02x}{})", info.page_id, info.usage_id, usage_names)
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
        _ => format!("{}", event.value),
    };
    let mut result = format!("Event:  type:code = {:<13}  value = {}", name, value_str);

    if let Some(domain_name) = domain::try_reverse_resolve(event.domain) {
        result = format!("{:<53}  domain = {}", result, domain_name);
    }
    if event.code == EventCode::MSC_SCAN {
        result = format!("{:<80}  {}", result, format_hidinfo(event.value));
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