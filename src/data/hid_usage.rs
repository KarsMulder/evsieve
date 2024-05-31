// SPDX-License-Identifier: GPL-2.0-or-later

use crate::event::EventValue;
use crate::data::hid_usage_tables::HID_PAGES;

pub struct HidPage {
    pub id: u16,
    pub name: &'static str,
    pub usages: &'static [HidUsage],
}

pub struct HidUsage {
    pub id: u16,
    pub name: &'static str,
}

/// Returned from `get_usage_from_scancode()`.
pub struct UsageInfo {
    pub page_id: u16,
    pub usage_id: u16,
    pub names: UsageNames,
}

pub enum UsageNames {
    /// We do not know anything about the usage of this scancode.
    Unknown,
    /// We know to which page this scancode belongs.
    PageKnown { page_name: &'static str },
    /// We know both the page and usage of this scancode.
    Known { page_name: &'static str, usage_name: &'static str },
}

/// Assumes that a scancode is composed of a HID usage page and a HID usage value as defined by the
/// USB specification. Tries to look up the names of the usage page and usage value.
pub fn get_usage_from_scancode(scancode: EventValue) -> UsageInfo {
    let scancode_u32 = scancode as u32;
    let page_id: u16 = ((scancode_u32 & 0xffff0000) >> 16) as u16;
    let usage_id: u16 = (scancode_u32 & 0x0000ffff) as u16;

    match HID_PAGES.binary_search_by_key(&page_id, |page| page.id) {
        Err(_) => UsageInfo { page_id, usage_id, names: UsageNames::Unknown },
        Ok(page_idx) => {
            let page = &HID_PAGES[page_idx];
            match page.usages.binary_search_by_key(&usage_id, |usage| usage.id) {
                Err(_) => UsageInfo { page_id, usage_id, names: UsageNames::PageKnown { page_name: page.name } },
                Ok(usage_idx) => {
                    let usage = &page.usages[usage_idx];
                    UsageInfo { page_id, usage_id, names: UsageNames::Known { page_name: page.name, usage_name: usage.name} }
                }
            }
        }
    }
}
