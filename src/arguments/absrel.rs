// SPDX-License-Identifier: GPL-2.0-or-later

use crate::error::ArgumentError;
use crate::arguments::lib::ComplexArgGroup;
use crate::event::EventType;
use crate::key::{Key, KeyParser};
use crate::range::Range;
use crate::stream::absrel::RelToAbs;

/// Represents a --rel-to-abs argument.
pub(super) struct RelToAbsArg {
    pub input_key: Key,
    /// The output key should be a pure key.
    pub output_key: Key,
    pub output_range: Range,
    pub speed: f64,
}

impl RelToAbsArg {
	pub fn parse(args: Vec<String>) -> Result<RelToAbsArg, ArgumentError> {
        let arg_group = ComplexArgGroup::parse(args,
            &[],
            &["speed"],
            false,
            true,
        )?;

        let mut parser = KeyParser::default_filter();
        parser.type_whitelist = Some(vec![EventType::REL]);
        let keys = parser.parse_all(&arg_group.get_keys_or_empty_key())?;

        let (input_key, output_key) = match keys.as_slice() {
            [a, b] => (a.clone(), b.clone()),
            _ => return Err(ArgumentError::new("The --rel-to-abs argument needs to be provided exactly two keys, the first one matching the rel events that get mapped and the second matching the target abs event.")),
        };

        if input_key.requires_event_type() != Some(EventType::REL) {
            return Err(ArgumentError::new("The first key provided to --rel-to-abs must match events of type rel, e.g.: --rel-to-abs rel:x abs:x:0~255"));
        }
        if output_key.requires_event_type() != Some(EventType::ABS) {
            return Err(ArgumentError::new("The second key provided to --rel-to-abs must match events of type abs, e.g.: --rel-to-abs rel:x abs:x:0~255"));
        }

        let (output_key, output_range_opt) = output_key.split_value();
        let output_range = match output_range_opt {
            Some(range) => range,
            None => return Err(ArgumentError::new(
                "You need to provide a range for the possible output values of the --abs-to-rel argument. For example, \"--abs-to-rel rel:x abs:x:0~255\" will ensure that the outputted values for abs:x stay between 0 and 255."
            )),
        };

        let speed = match arg_group.get_unique_clause("speed")? {
            Some(speed_str) => match speed_str.parse() {
                Ok(value) => value,
                // TODO: Use a more stringent parser
                Err(err) => return Err(ArgumentError::new(
                    format!("Cannot parse the speed of \"{}\" as a number: {}", speed_str, err)
                )),
            },
            None => 1.0,
        };

        Ok(RelToAbsArg { input_key, output_key, output_range, speed })
    }


    pub fn compile(self) -> RelToAbs {
        RelToAbs::new(self.input_key, self.output_key, self.output_range, self.speed)
    }
}