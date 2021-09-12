// SPDX-License-Identifier: GPL-2.0-or-later

use crate::error::ArgumentError;
use crate::arguments::lib::ComplexArgGroup;
use crate::key::{Key, KeyParser};
use crate::event::Namespace;
use crate::map::ToggleMode;

/// Represents a --toggle argument.
pub(super) struct ToggleArg {
	pub input_key: Key,
    pub output_keys: Vec<Key>,
    pub id: Option<String>,
    pub mode: ToggleMode,
}

impl ToggleArg {
	pub fn parse(args: Vec<String>) -> Result<ToggleArg, ArgumentError> {
        let arg_group = ComplexArgGroup::parse(args,
            &[],
            &["id", "mode"],
            false,
            true,
        )?;

        let mode = match arg_group.get_unique_clause("mode")? {
            None => ToggleMode::Consistent,
            Some(mode_str) => match mode_str.as_str() {
                "consistent" => ToggleMode::Consistent,
                "passive" => ToggleMode::Passive,
                _ => return Err(ArgumentError::new(
                    format!("Invalid toggle mode specified: {}", mode_str)
                ))
            }
        };

        let keys = arg_group.require_keys()?;
        if keys.len() < 2 {
            return Err(ArgumentError::new("A --toggle argument requires an input key and at least one output key."));
        }

        let input_key = KeyParser {
            allow_transitions: true,
            allow_ranges: true,
            default_value: "",
            allow_types: true,
            namespace: Namespace::User,
        }.parse(&keys[0])?;
    
        let output_keys = KeyParser {
            allow_ranges: false,
            allow_transitions: false,
            default_value: "",
            allow_types: false,
            namespace: Namespace::User,
        }.parse_all(&keys[1..])?;

        let id = arg_group.get_unique_clause("id")?;
        if let Some(id) = &id {
            if id.contains(':') {
                return Err(ArgumentError::new(format!("A toggle's id cannot contain any colons. Offending id: {}", id)));
            }
        }
        
        Ok(ToggleArg {
            input_key, output_keys, mode, id
        })
    }

    // The size of the ToggleState we need to reserve for this toggle.
    pub fn size(&self) -> usize {
        self.output_keys.len()
    }
}