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

        Ok(InputDevice {
            domain, grab_mode, persist_mode,
            paths: arg_group.require_paths()?,
        })
    }
}