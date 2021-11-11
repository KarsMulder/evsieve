// SPDX-License-Identifier: GPL-2.0-or-later

use crate::error::ArgumentError;
use crate::arguments::lib::ComplexArgGroup;
use crate::key::{Key, KeyParser};
use crate::merge::Merge;
use crate::event::{EventType, Namespace};

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

        let keys = if arg_group.keys.is_empty() {
            vec![Key::from_ev_type_and_namespace(EventType::KEY, Namespace::User)]
        } else {
            let parser = KeyParser {
                default_value: "",
                allow_values: false,
                allow_ranges: false,
                allow_transitions: false,
                allow_types: true,
                restrict_to_EV_KEY: true,
                namespace: crate::event::Namespace::User,
            };

            parser.parse_all(&arg_group.keys)?
        };

        Ok(MergeArg { keys })
    }

    pub fn compile(self) -> Merge {
        Merge::new(self.keys)
    }
}