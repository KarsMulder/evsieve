// SPDX-License-Identifier: GPL-2.0-or-later

use crate::error::{ArgumentError, RuntimeError};
use crate::event::EventType;
use crate::key::{Key, KeyParser};
use crate::stream::scale::Scale;

use super::lib::ComplexArgGroup;

/// Represents a --scale argument.
pub(super) struct ScaleArg {
	pub input_keys: Vec<Key>,
    
    // I have deemed it acceptable for this to be a f64 based on some reasons: (1) maps use f64 too, (2) common fractions
    // that users want to be exact such as x0.5, x0.25 and such can be represented as float, (3) using a custom Rational
    // type would also cause errors when a decimal number such as 0.33333333333333 gets converted to Rational.
    pub factor: f64,
}

impl ScaleArg {
	pub fn parse(args: Vec<String>) -> Result<ScaleArg, RuntimeError> {
        let arg_group = ComplexArgGroup::parse(args,
            &[],
            &["factor"],
            false,
            true,
        )?;

        // Parse the keys.
        let keys_str = arg_group.get_keys_or_empty_key();
        let mut parser = KeyParser::default_filter();
        // IMPORTANT: the blanket keys like "" are also accepted by type_whitelist. This is intentional and allows
        // stuff like "--scale @foo factor=2" without more obnoxious stuff. --scale only applies to events of type
        // abs or rel, even in case of blanket keys like "".
        parser.type_whitelist = Some(vec![EventType::REL, EventType::ABS]);
        let input_keys = parser.parse_all(&keys_str)?;

        let factor_str = arg_group.require_unique_clause("factor")?;
        let factor = crate::utils::parse_number(&factor_str)
            .ok_or_else(|| ArgumentError::new(format!("Cannot interpret the factor \"{}\" as a number.", factor_str)))?;

        Ok(ScaleArg { input_keys, factor })
    }

    pub fn compile(self) -> Scale {
        Scale::new(self.input_keys, self.factor)
    }
}
