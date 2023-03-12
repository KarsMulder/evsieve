// SPDX-License-Identifier: GPL-2.0-or-later

use crate::error::{ArgumentError};
use crate::arguments::lib::ComplexArgGroup;

/// Represents a --map or --copy argument.
pub(super) struct ConfigArg {
    pub paths: Vec<String>,
}

impl ConfigArg {
	pub fn parse(args: Vec<String>) -> Result<ConfigArg, ArgumentError> {
        let arg_group = ComplexArgGroup::parse(args,
            &[],
            &[],
            true,
            false,
        )?;

        Ok(ConfigArg { paths: arg_group.paths })
    }
}

// TODO: FEATURE(config) Replace this with a proper lexer.
pub fn shell_lex(input: String) -> Result<Vec<String>, ArgumentError> {
    Ok(input.split_whitespace().filter(|token| !token.is_empty()).map(String::from).collect())
}
