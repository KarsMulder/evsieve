// SPDX-License-Identifier: GPL-2.0-or-later

use crate::error::ArgumentError;
use crate::arguments::lib::ComplexArgGroup;
use crate::event::EventType;
use crate::key::{Key, KeyParser};
use crate::stream::absrel::RelToAbs;

/// Represents a --rel-to-abs argument.
pub(super) struct RelToAbsArg {
    pub keys: Vec<Key>,
}

impl RelToAbsArg {
	pub fn parse(args: Vec<String>) -> Result<RelToAbsArg, ArgumentError> {
        let arg_group = ComplexArgGroup::parse(args,
            &[],
            &[],
            false,
            true,
        )?;

        let mut parser = KeyParser::default_filter();
        parser.type_whitelist = Some(vec![EventType::REL]);
        let keys = parser.parse_all(&arg_group.get_keys_or_empty_key())?;

        Ok(RelToAbsArg { keys })
    }

    pub fn compile(self) -> RelToAbs {
        RelToAbs::new(self.keys)
    }
}