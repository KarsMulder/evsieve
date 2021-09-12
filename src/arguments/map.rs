// SPDX-License-Identifier: GPL-2.0-or-later

use crate::error::{ArgumentError, InternalError, RuntimeError};
use crate::arguments::lib::ComplexArgGroup;
use crate::key::{Key, KeyParser};
use crate::event::Namespace;

/// Represents a --map or --copy argument.
pub(super) struct MapArg {
	pub input_key: Key,
    pub output_keys: Vec<Key>,
}

impl MapArg {
	pub fn parse(args: Vec<String>) -> Result<MapArg, RuntimeError> {
        let arg_group = ComplexArgGroup::parse(args,
            &["yield"],
            &[],
            false,
            true,
        )?;

        let copy = match arg_group.name.as_str() {
            "--copy" => true,
            "--map" => false,
            _ => return Err(InternalError::new("A map has been constructed from neither a --map or a --copy.").into()),
        };

        // Parse the keys.
        let keys_str = arg_group.require_keys()?;
        let input_key = KeyParser {
            allow_transitions: true,
            allow_ranges: true,
            allow_types: true,
            default_value: "",
            namespace: Namespace::User,
        }.parse(&keys_str[0])?;
        
        let output_namespace = match arg_group.has_flag("yield") {
            true => Namespace::Yielded,
            false => Namespace::User,
        };
        let mut output_keys = KeyParser {
            allow_ranges: false,
            allow_transitions: false,
            allow_types: false,
            default_value: "",
            namespace: output_namespace,
        }.parse_all(&keys_str[1..])?;

        if copy {
            output_keys.insert(0, Key::copy());
        }
        
        Ok(MapArg {
            input_key, output_keys,
        })
    }
}

/// Represents a --block argument.
pub(super) struct BlockArg {
	pub keys: Vec<Key>,
}

impl BlockArg {
	pub fn parse(args: Vec<String>) -> Result<BlockArg, ArgumentError> {
        let arg_group = ComplexArgGroup::parse(args,
            &[],
            &[],
            false,
            true,
        )?;

        let keys = KeyParser {
            allow_ranges: true,
            allow_transitions: true,
            allow_types: true,
            default_value: "",
            namespace: Namespace::User,
        }.parse_all(&arg_group.get_keys_or_empty_key())?;

        Ok(BlockArg { keys })
    }
}