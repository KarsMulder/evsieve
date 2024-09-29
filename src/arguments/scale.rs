// SPDX-License-Identifier: GPL-2.0-or-later

use crate::error::{ArgumentError, RuntimeError};
use crate::event::EventType;
use crate::key::{Key, KeyParser};
use crate::stream::scale::Scale;

use super::lib::ComplexArgGroup;

/// Represents a --scale argument.
pub(super) struct ScaleArg {
	pub input_keys: Vec<Key>,
    // TODO (High Priority): shouldn't this be a rational?
    pub factor: f64,
}

// TODO (High Priority): figure out how (and with which rounding modes) we want --scale to apply to EV_ABS-type events.
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
