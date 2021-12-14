// SPDX-License-Identifier: GPL-2.0-or-later

use crate::domain;
use crate::domain::Domain;
use crate::predevice::{GrabMode, PersistMode};
use crate::error::ArgumentError;
use crate::arguments::lib::ComplexArgGroup;

/// Represents an --input argument.
pub(super) struct InputDevice {
    /// The domain of this input device.
    pub domain: Option<Domain>,
    /// All input device paths. If multiple are specified, it will read from multiple devices.
    /// At least one path must be specified.
	pub paths: Vec<String>,
    pub grab_mode: GrabMode,
    pub persist_mode: PersistMode,
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
                _ => return Err(ArgumentError::new("Invalid persist mode specified.")),
            }
        };

        let paths = arg_group.require_paths()?;

        match persist_mode {
            PersistMode::None => {},
            PersistMode::Reopen => {
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

/// Returns true if `path` is of the form `^/dev/input/event[0-9]+$`.
fn is_direct_event_device(path: &str) -> bool {
    let path = match crate::utils::strip_prefix(path, "/dev/input/event") {
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