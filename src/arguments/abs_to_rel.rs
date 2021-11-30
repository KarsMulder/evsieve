// SPDX-License-Identifier: GPL-2.0-or-later

// TODO: add this to the documentation and usage message.

use crate::error::{RuntimeError};
use crate::arguments::lib::ComplexArgGroup;
use crate::key::{Key, KeyParser};

/// Represents a --abs-to-rel argument.
pub(super) struct AbsToRelArg {
	pub reset_keys: Vec<Key>,
}

impl AbsToRelArg {
	pub fn parse(args: Vec<String>) -> Result<Self, RuntimeError> {
        let arg_group = ComplexArgGroup::parse(args,
            &[],
            &["reset"],
            false,
            false,
        )?;

        let reset_keys = arg_group.get_clauses("reset");
        let reset_keys = KeyParser::default_filter().parse_all(&reset_keys)?;
        
        Ok(Self {
            reset_keys,
        })
    }
}
