// SPDX licence header intentionally missing.
//
// To the extent I own the copyright on this file, it is licensed under "GPL-2.0-or-later".
// However, I am not a lawyer and not certain who owns the "key code -> scancode" table or
// whether it is copyrightable at all. It is probably derived from the USB standard. The USB 
// specification on HID Usage Tables mentions 
//
//     It is contemplated that many implementations of this specification (e.g., in a product)
//     do not require a license to use this specification under copyright. For clarity,
//     however, to the maximum extent of usb implementers forum’s rights, usb
//     implementers forum hereby grants a license under copyright to use this specification
//     as reasonably necessary to implement this specification (e.g., in a product).
//
// The USB specification can be obtained from https://usb.org/document-library/hid-usage-tables-13
// (retrieved April 29th, 2022.)
//
// Since the Linux kernel clearly contains the following table, we can hopefully assume that
// it is compatible with at least "GPL-2.0-only WITH Linux-syscall-note".

use std::collections::HashMap;
use crate::ecodes;
use crate::event::EventCode;

pub type Scancode = i32;

lazy_static! {
    static ref SCANCODES: HashMap<EventCode, Scancode> = {
        // TODO: LOW-PRIORITY: the following table is still incomplete and possibly incorrect.
        let hardcoded_scancodes: &[(&'static str, Scancode)] = &[
            (&"key:a", 458756),
            (&"key:b", 458757),
            (&"key:c", 458758),
            (&"key:d", 458759),
            (&"key:e", 458760),
            (&"key:f", 458761),
            (&"key:g", 458762),
            (&"key:h", 458763),
            (&"key:i", 458764),
            (&"key:j", 458765),
            (&"key:k", 458766),
            (&"key:l", 458767),
            (&"key:m", 458768),
            (&"key:n", 458769),
            (&"key:o", 458770),
            (&"key:p", 458771),
            (&"key:q", 458772),
            (&"key:r", 458773),
            (&"key:s", 458774),
            (&"key:t", 458775),
            (&"key:u", 458776),
            (&"key:v", 458777),
            (&"key:w", 458778),
            (&"key:x", 458779),
            (&"key:y", 458780),
            (&"key:z", 458781),
            (&"key:1", 458782),
            (&"key:2", 458783),
            (&"key:3", 458784),
            (&"key:4", 458785),
            (&"key:5", 458786),
            (&"key:6", 458787),
            (&"key:7", 458788),
            (&"key:8", 458789),
            (&"key:9", 458790),
            (&"key:0", 458791),
            (&"key:enter", 458792),
            (&"key:esc", 458793),
            (&"key:backspace", 458794),
            (&"key:tab", 458795),
            (&"key:space", 458796),
            (&"key:minus", 458797),
            (&"key:equal", 458798),
            (&"key:leftbrace", 458799),
            (&"key:rightbrace", 458800),
            (&"key:backslash", 458801),
            (&"key:semicolon", 458803),
            (&"key:apostrophe", 458804),
            (&"key:grave", 458805),
            (&"key:comma", 458806),
            (&"key:dot", 458807),
            (&"key:slash", 458808),
            (&"key:capslock", 458809),
            (&"key:f1", 458810),
            (&"key:f2", 458811),
            (&"key:f3", 458812),
            (&"key:f4", 458813),
            (&"key:f5", 458814),
            (&"key:f6", 458815),
            (&"key:f7", 458816),
            (&"key:f8", 458817),
            (&"key:f9", 458818),
            (&"key:f10", 458819),
            (&"key:f11", 458820),
            (&"key:f12", 458821),
            (&"key:sysrq", 458822),
            (&"key:scrolllock", 458823),
            (&"key:pause", 458824),
            (&"key:insert", 458825),
            (&"key:home", 458826),
            (&"key:pageup", 458827),
            (&"key:delete", 458828),
            (&"key:end", 458829),
            (&"key:pagedown", 458830),
            (&"key:right", 458831),
            (&"key:left", 458832),
            (&"key:down", 458833),
            (&"key:up", 458834),
            (&"key:numlock", 458835),
            (&"key:kpslash", 458836),
            (&"key:kpasterisk", 458837),
            (&"key:kpminus", 458838),
            (&"key:kpplus", 458839),
            (&"key:kpenter", 458840),
            (&"key:kp1", 458841),
            (&"key:kp2", 458842),
            (&"key:kp3", 458843),
            (&"key:kp4", 458844),
            (&"key:kp5", 458845),
            (&"key:kp6", 458846),
            (&"key:kp7", 458847),
            (&"key:kp8", 458848),
            (&"key:kp9", 458849),
            (&"key:kp0", 458850),
            (&"key:kpdot", 458851),
            (&"key:compose", 458853),
            (&"key:leftctrl", 458976),
            (&"key:leftshift", 458977),
            (&"key:leftalt", 458978),
            (&"key:leftmeta", 458979),
            (&"key:rightctrl", 458980),
            (&"key:rightshift", 458981),
            (&"key:rightalt", 458982),
        ];

        hardcoded_scancodes.into_iter().filter_map(|(key_str, scancode)| {
            let (type_name, code_name_opt) = crate::utils::split_once(key_str, ":");
            let code_name = code_name_opt.unwrap(); // Unwrap ok: data is hardcoded.

            // We defensively check for None here because whether these codes exist might
            // depend on the version of libevdev we link against.
            if let Ok(event_code) = ecodes::event_code(type_name, code_name) {
                Some((event_code, scancode.clone()))
            } else {
                None
            }
        }).collect()
    };
}

pub fn from_event_code(code: EventCode) -> Option<Scancode> {
    SCANCODES.get(&code).cloned()
}
