// SPDX-License-Identifier: GPL-2.0-or-later

use crate::key::Key;
use crate::event::{Event, EventType, EventValue, EventCode};
use crate::ecodes;
use crate::domain;

use hut::{Usage, UsagePage};

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

fn cleanhid(s: String) -> String {
  return s.replace(" ", "").replace("-", "").replace("/", "");
}

fn hidstring(ev: EventValue) -> String {
    let v = i32::from(ev) as u32;
    let usage_page = ((v & 0xffff0000) >> 16 ) as u16;
    let usage_id = (v & 0x0000ffff) as u16;
    let usage_dump_str = format!("(hid usage page 0x{:x} id 0x{:x})", usage_page, usage_id);

    let maybe_usage = Usage::new_from_page_and_id(usage_page, usage_id);
    let maybe_usagepage = UsagePage::from_usage_page_value(usage_page);

    if let Some(usage) = maybe_usage.ok() {
        return format!("{}/{} {}", cleanhid(maybe_usagepage.unwrap().to_string()), cleanhid(usage.to_string()), usage_dump_str);
    }

    if let Some(usagepage) = maybe_usagepage.ok() {
        return format!("{} {}", cleanhid(usagepage.to_string()), usage_dump_str);
    }

    return format!("{}", usage_dump_str);
}

pub fn print_event_detailed(event: Event) -> String {
    let name = ecodes::event_name(event.code);
    let value_str = match event.ev_type() {
        EventType::KEY => match event.value {
            0 => "0x0 (up)".to_string(),
            1 => "0x1 (down)".to_string(),
            2 => "0x2 (repeat)".to_string(),
            _ => format!("0x{:x}", event.value),
        },
        _ => format!("0x{:x}", event.value),
    };

    // keyboard scan codes are often USB Usage Pages and IDs, so show those too
    let is_scancode = event.ev_type() == EventType::MSC && event.code == EventCode::MSC_SCAN;
    let hid_str = if !is_scancode { "".to_string() } else { format!("hid = {}", hidstring(event.value)) };

    let name_and_value = format!("Event:  type:code = {:<13} value = {} {}", name, value_str, hid_str);

    if let Some(domain_name) = domain::try_reverse_resolve(event.domain) {
        format!("{:<53}  domain = {}", name_and_value, domain_name)
    } else {
        name_and_value
    }
}

pub fn print_event_direct(event: Event) -> String {
    let name = ecodes::event_name(event.code);
    if let Some(domain_name) = domain::try_reverse_resolve(event.domain) {
        format!("{}:{}@{}", name, event.value, domain_name)
    } else {
        format!("{}:{}", name, event.value)
    }
}