// SPDX-License-Identifier: GPL-2.0-or-later

// TODO: add this to the documentation and usage message.

use crate::error::{RuntimeError, ArgumentError};
use crate::arguments::lib::ComplexArgGroup;
use crate::key::{Key, KeyParser};

/// Represents a --abs-to-rel argument.
pub(super) struct AbsToRelArg {
	pub reset_keys: Vec<Key>,
    pub speed: f64,
}

impl AbsToRelArg {
	pub fn parse(args: Vec<String>) -> Result<Self, RuntimeError> {
        let arg_group = ComplexArgGroup::parse(args,
            &[],
            &["reset", "speed"],
            false,
            false,
        )?;

        let reset_keys = arg_group.get_clauses("reset");
        let reset_keys = KeyParser::default_filter().parse_all(&reset_keys)?;

        let speed = match arg_group.get_unique_clause("speed")? {
            None => 1.0,
            Some(value) => match value.parse::<f64>() {
                Ok(value) => value,
                Err(_error) => return Err(ArgumentError::new(
                    "The speed parameter needs to be a number, e.g. \"speed=2\" or \"speed=0.25\".".to_string()
                ).into())
            }
        };
        
        Ok(Self {
            reset_keys, speed
        })
    }
}
