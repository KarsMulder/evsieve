// SPDX-License-Identifier: GPL-2.0-or-later

use crate::error::ArgumentError;
use crate::arguments::lib::ComplexArgGroup;
use crate::key::{Key, KeyParser};
use crate::merge::Merge;

/// Represents a --merge argument.
pub(super) struct MergeArg {
    pub keys: Vec<Key>,
}

impl MergeArg {
	pub fn parse(args: Vec<String>) -> Result<MergeArg, ArgumentError> {
        let arg_group = ComplexArgGroup::parse(args,
            &[],
            &[],
            false,
            true,
        )?;

        let keys = KeyParser::default_filter().parse_all(&arg_group.get_keys_or_empty_key())?;

        Ok(MergeArg { keys })
    }

    pub fn compile(self) -> Merge {
        unimplemented!()
    }
}