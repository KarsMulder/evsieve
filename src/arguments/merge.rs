// SPDX-License-Identifier: GPL-2.0-or-later

use crate::error::ArgumentError;
use crate::arguments::lib::ComplexArgGroup;
use crate::event::EventType;
use crate::key::{Key, KeyParser};
use crate::stream::merge::Merge;

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

        let parser = KeyParser {
            default_value: "",
            allow_values: false,
            allow_ranges: false,
            allow_domains: true,
            allow_transitions: false,
            allow_types: true,
            allow_relative_values: false,
            type_whitelist: Some(vec![EventType::KEY]),
            namespace: crate::event::Namespace::User,
        };

        let keys: Vec<Key> = parser.parse_all(&arg_group.get_keys_or_empty_key())?;

        Ok(MergeArg { keys })
    }

    pub fn compile(self) -> Merge {
        Merge::new(self.keys)
    }
}

#[test]
fn unittest() {
    assert!(MergeArg::parse(vec!["--merge".to_string()]).is_ok());
    assert!(MergeArg::parse(vec!["--merge".to_string(), "".to_string()]).is_ok());
    assert!(MergeArg::parse(vec!["--merge".to_string(), "key".to_string()]).is_ok());
    assert!(MergeArg::parse(vec!["--merge".to_string(), "key:a".to_string()]).is_ok());
    assert!(MergeArg::parse(vec!["--merge".to_string(), "btn".to_string()]).is_ok());
    assert!(MergeArg::parse(vec!["--merge".to_string(), "btn:left".to_string()]).is_ok());
    assert!(MergeArg::parse(vec!["--merge".to_string(), "key".to_string(), "btn".to_string()]).is_ok());
    assert!(MergeArg::parse(vec!["--merge".to_string(), "@foo".to_string()]).is_ok());

    assert!(MergeArg::parse(vec!["--merge".to_string(), "key:a:1".to_string()]).is_err());
    assert!(MergeArg::parse(vec!["--merge".to_string(), "abs".to_string()]).is_err());
    assert!(MergeArg::parse(vec!["--merge".to_string(), "abs:x".to_string()]).is_err());
    assert!(MergeArg::parse(vec!["--merge".to_string(), "key".to_string(), "abs".to_string()]).is_err());
    assert!(MergeArg::parse(vec!["--merge".to_string(), "abs@foo".to_string()]).is_err());
}