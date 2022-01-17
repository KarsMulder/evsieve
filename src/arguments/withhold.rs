// SPDX-License-Identifier: GPL-2.0-or-later

use crate::error::ArgumentError;
use crate::arguments::lib::ComplexArgGroup;
use crate::key::{Key, KeyParser};

/// Represents a --withhold argument.
pub(super) struct WithholdArg {
    pub keys: Vec<Key>,
}

impl WithholdArg {
	pub fn parse(args: Vec<String>) -> Result<WithholdArg, ArgumentError> {
        let arg_group = ComplexArgGroup::parse(args,
            &[],
            &[],
            false,
            true,
        )?;

        // TODO: accept only pure keys.
        let keys = KeyParser::default_filter()
            .parse_all(&arg_group.get_keys_or_empty_key())?;

        Ok(WithholdArg { keys })
    }
}
