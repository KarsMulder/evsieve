// SPDX-License-Identifier: GPL-2.0-or-later

use std::path::Path;
use crate::domain;
use crate::domain::Domain;
use crate::persist::storage::DeviceCache;
use crate::predevice::{GrabMode, PersistState};
use crate::error::{ArgumentError, SystemError};
use crate::arguments::lib::ComplexArgGroup;

/// Represents an --input argument.
pub(super) struct InputDevice {
    /// The domain of this input device.
    pub domain: Option<Domain>,
    /// All input device paths. If multiple are specified, it will read from multiple devices.
    /// At least one path must be specified.
    /// TODO (Low Priority): Consider adding a newtype InputDevicePath for extra type safety.
	pub paths: Vec<String>,
    pub grab_mode: GrabMode,
    pub persist_mode: PersistMode,
}

#[derive(Clone, Copy)]
pub enum PersistMode {
    /// Remove the device from the processing stream at runtime, or throw an error at startup time.
    None,
    /// Try to reattach the device at runtime, or throw an error at startup time.
    Reopen,
    /// Try to reattach the device at runtime. If at startup time the device is not available, use
    /// the cached capabilities of it. Cache the capabilities of this device.
    Full,
    /// If a device with mode exit disconnects, evsieve shall exit, even if other devices are still available.
    Exit,
}

impl InputDevice {
	pub fn parse(args: Vec<String>) -> Result<InputDevice, ArgumentError> {
        let arg_group = ComplexArgGroup::parse(args,
            &["grab"],
            &["domain", "grab", "persist"],
            true,
            false,
        )?;

        let domain = match arg_group.get_unique_clause("domain")? {
            None => None,
            Some(domain_str) => {
                let mut chars = domain_str.chars();
                let first_char: Option<char> = chars.next();
                let later_chars: String = chars.collect();
                match first_char {
                    Some('@') => return Err(ArgumentError::new(format!("There must be no @ in the domain name from \"domain={}\", because \"@{}\" represents a filter meaning \"any event with domain {}\". Try specifying \"domain={}\" instead.", domain_str, later_chars, later_chars, later_chars))),
                    None => return Err(ArgumentError::new("The domain= clause of an input argument cannot be empty.")),
                    _ => (),
                };
                Some(domain::resolve(&domain_str)?)
            }
        };

        let grab_mode = match arg_group.get_unique_clause_or_default_if_flag("grab", "auto")? {
            None => GrabMode::None,
            Some(value) => match value.as_str() {
                "auto" => GrabMode::Auto,
                "force" => GrabMode::Force,
                _ => return Err(ArgumentError::new("Invalid grab mode specified.")),
            }
        };

        let persist_mode = match arg_group.get_unique_clause("persist")? {
            None => PersistMode::None,
            Some(value) => match value.as_str() {
                "reopen" => PersistMode::Reopen,
                "none" => PersistMode::None,
                "exit" => PersistMode::Exit,
                "full" => PersistMode::Full,
                _ => return Err(ArgumentError::new("Invalid persist mode specified.")),
            }
        };

        let paths = arg_group.require_paths()?;

        match persist_mode {
            PersistMode::None | PersistMode::Exit => {},
            PersistMode::Reopen | PersistMode::Full => {
                if paths.iter().any(|path| is_direct_event_device(path)) {
                    println!("Warning: it is a bad idea to enable persistence on paths like /dev/input/event* because the kernel does not guarantee that the number of each event device remains constant. If such a device were to de disattached and reattached, it may show up under a different number. We recommend identifying event devices through their links in /dev/input/by-id/.");
                }
            }
        }

        Ok(InputDevice {
            domain, grab_mode, persist_mode, paths
        })
    }
}

impl PersistMode {
    pub fn to_state_for_device(self, input_device_path: &Path) -> Result<PersistState, SystemError> {
        Ok(match self {
            PersistMode::Exit => PersistState::Exit,
            PersistMode::None => PersistState::None,
            PersistMode::Reopen => PersistState::Reopen,
            PersistMode::Full => PersistState::Full(
                DeviceCache::load_for_input_device(input_device_path)?
            )
        })
    }
}

/// Returns true if `path` is of the form `^/dev/input/event[0-9]+$`.
fn is_direct_event_device(path: &str) -> bool {
    let path = match path.strip_prefix("/dev/input/event") {
        Some(string) => string,
        None => return false,
    };

    path.chars().all(char::is_numeric)
}

#[test]
fn unittest() {
    assert!(is_direct_event_device("/dev/input/event1"));
    assert!(is_direct_event_device("/dev/input/event23"));
    assert!(! is_direct_event_device("/dev/input/by-id/event23"));
    assert!(! is_direct_event_device("/dev/input/event1foo"));
}