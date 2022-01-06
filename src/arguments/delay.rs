// SPDX-License-Identifier: GPL-2.0-or-later

use crate::error::ArgumentError;
use crate::arguments::lib::ComplexArgGroup;
use crate::key::{Key, KeyParser};
use crate::delay::Delay;
use std::time::Duration;

/// Represents a --delay argument.
pub(super) struct DelayArg {
    pub keys: Vec<Key>,
    pub period: Duration,
}

impl DelayArg {
	pub fn parse(args: Vec<String>) -> Result<DelayArg, ArgumentError> {
        let arg_group = ComplexArgGroup::parse(args,
            &[],
            &["period"],
            false,
            true,
        )?;

        let keys = KeyParser::default_filter()
            .parse_all(&arg_group.get_keys_or_empty_key())?;
        
        // TODO: refactor into require_unique_clause
        let period = match arg_group.get_unique_clause("period")? {
            Some(value) => parse_period_value(&value)?,
            None => return Err(ArgumentError::new(
                "The --delay argument requires a period= clause, e.g. use --delay period=0.5 to delay all events by half a second."
            ))
        };

        Ok(DelayArg { keys, period })
    }

    pub fn compile(self) -> Delay {
        Delay::new(self.keys, self.period)
    }
}

fn parse_period_value(value: &str) -> Result<Duration, ArgumentError> {
    match crate::utils::parse_number(&value) {
        Some(seconds) => {
            if seconds == 0.0 {
                return Err(ArgumentError::new("The period must be nonzero."));
            } else if seconds < 0.0 {
                return Err(ArgumentError::new("The period must be nonnegative."));
            } else {
                Ok(Duration::from_secs_f64(seconds))
            }
        },
        None => return Err(ArgumentError::new(format!(
            "Cannot interpret {} as a number. The period must be a number of seconds.", value
        )))
    }
}