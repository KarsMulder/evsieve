// SPDX-License-Identifier: GPL-2.0-or-later

use crate::error::ArgumentError;
use crate::arguments::lib::ComplexArgGroup;
use crate::event::EventType;
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

        let parser = KeyParser {
            default_value: "",
            allow_values: false,
            allow_ranges: false,
            allow_transitions: false,
            allow_types: true,
            allow_relative_values: false,
            namespace: crate::event::Namespace::User,
        };

        let mut keys: Vec<Key> = Vec::new();
        for key_str in arg_group.get_keys_or_empty_key() {
            let key = parser.parse(&key_str)?;
            match key.requires_event_type() {
                None | Some(EventType::KEY) => keys.push(key),
                Some(_) => return Err(ArgumentError::new(format!(
                    "The --merge argument is only applicable to EV_KEY type events (\"key:something\" or \"btn:something\"). As such, it does not make sense to give it the key \"{}\".",
                    key_str
                )))
            }
        }

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