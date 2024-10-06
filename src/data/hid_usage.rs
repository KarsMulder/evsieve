// SPDX-License-Identifier: GPL-2.0-or-later

use std::sync::OnceLock;

use crate::event::EventValue;
pub struct HidPage {
    pub id: u16,
    pub name: String,
    pub usages: Vec<HidUsage>,
}

pub struct HidUsage {
    pub id: u16,
    pub name: String,
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

pub enum UsagePagesState {
    /// The user either doesn't have the necessary packages installed, or we failed to load the pages.
    NotAvailable,
    /// The HID usage pages were found
    Available(Vec<HidPage>),
}

pub static HID_PAGES: OnceLock<UsagePagesState> = OnceLock::new();

/// Declares that we may need the HID pages some time in the future, and we should already start loading them
/// so we don't possibly hit the hard drive later, slowing down event processing.
pub fn preload_hid_pages() {
    HID_PAGES.get_or_init(|| super::hid_usage_parser::load_tables_and_print_error());
}

impl UsagePagesState {
    /// Assumes that a scancode is composed of a HID usage page and a HID usage value as defined by the
    /// USB specification. Tries to look up the names of the usage page and usage value.
    /// 
    /// Returns None if the usage pages have not been loaded.
    pub fn get_usage_from_scancode(&'static self, scancode: EventValue) -> Option<UsageInfo> {
        let scancode_u32 = scancode as u32;
        let page_id: u16 = ((scancode_u32 & 0xffff0000) >> 16) as u16;
        let usage_id: u16 = (scancode_u32 & 0x0000ffff) as u16;

        let UsagePagesState::Available(pages) = self else { return None };

        match pages.binary_search_by_key(&page_id, |page| page.id) {
            Err(_) => Some(UsageInfo { page_id, usage_id, names: UsageNames::Unknown }),
            Ok(page_idx) => {
                let page = &pages[page_idx];
                match page.usages.binary_search_by_key(&usage_id, |usage| usage.id) {
                    Err(_) => Some(UsageInfo { page_id, usage_id, names: UsageNames::PageKnown { page_name: &page.name } }),
                    Ok(usage_idx) => {
                        let usage = &page.usages[usage_idx];
                        Some(UsageInfo { page_id, usage_id, names: UsageNames::Known { page_name: &page.name, usage_name: &usage.name} })
                    }
                }
            }
        }
    }
}
