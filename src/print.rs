// SPDX-License-Identifier: GPL-2.0-or-later

use crate::key::Key;
use crate::event::{Event, EventType};
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

pub fn print_event_detailed(event: Event) -> String {
    let name = ecodes::event_name(event.ev_type, event.code);
    let value_str = match event.ev_type {
        EventType::KEY => match event.value {
            0 => "0 (up)".to_string(),
            1 => "1 (down)".to_string(),
            2 => "2 (repeat)".to_string(),
            _ => format!("{}", event.value),
        },
        _ => format!("{}", event.value),
    };
    let name_and_value = format!("Event:  type:code = {:<13}  value = {}", name, value_str);

    if let Some(domain_name) = domain::try_reverse_resolve(event.domain) {
        format!("{:<53}  domain = {}", name_and_value, domain_name)
    } else {
        name_and_value
    }
}

pub fn print_event_direct(event: Event) -> String {
    let name = ecodes::event_name(event.ev_type, event.code);
    if let Some(domain_name) = domain::try_reverse_resolve(event.domain) {
        format!("{}:{}@{}", name, event.value, domain_name)
    } else {
        format!("{}:{}", name, event.value)
    }
}