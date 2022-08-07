// SPDX-License-Identifier: GPL-2.0-or-later

use crate::error::ArgumentError;
use crate::arguments::lib::ComplexArgGroup;
use crate::key::{Key, KeyParser};
use crate::stream::delay::Delay;
use crate::time::Duration;

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
        
        let period = parse_period_value(
            &arg_group.require_unique_clause("period")?
        )?;

        Ok(DelayArg { keys, period })
    }

    pub fn compile(self) -> Delay {
        Delay::new(self.keys, self.period)
    }
}

/// Parses a number of seconds with up to nanosecond precision.
pub fn parse_period_value(value: &str) -> Result<Duration, ArgumentError> {
    let first_token = match value.chars().next() {
        Some(token) => token,
        None => return Err(ArgumentError::new("Empty period specified.")),
    };
    if first_token == '-' {
        return Err(ArgumentError::new("The period must be nonnegative."));
    }

    let (before_decimal, after_decimal) = crate::utils::split_once(value, ".");
    let seconds = before_decimal.parse::<u64>().map_err(|_| ArgumentError::new(format!(
        "Cannot interpret {} as a number.", value,
    )))?;

    // Compute the amount of nanoseconds after the period.
    let nanoseconds = match after_decimal {
        Some(string) => {
            let as_uint = string.parse::<u64>().map_err(|_| ArgumentError::new(format!(
                "Cannot interpret {} as a number.", value,
            )))?;
            let digits_after_period = string.len();
            if digits_after_period > 9 {
                return Err(ArgumentError::new("Cannot specify time periods with higher than nanosecond precision."));
            }
            as_uint * 10_u64.pow((9 - digits_after_period) as u32)
        },
        None => 0,
    };

    let total_nanoseconds: u64 = seconds * 1_000_000_000 + nanoseconds;
    if total_nanoseconds == 0 {
        return Err(ArgumentError::new("Cannot specify a period of zero."));
    }

    Ok(Duration::from_nanos(total_nanoseconds))
}

#[test]
fn unittest() {
    assert_eq!(parse_period_value("1").unwrap(), Duration::from_secs(1));
    assert_eq!(parse_period_value("5").unwrap(), Duration::from_secs(5));
    assert_eq!(parse_period_value("2.04").unwrap(), Duration::from_millis(2_040));
    assert_eq!(parse_period_value("0.049874").unwrap(), Duration::from_micros(49874));
    assert_eq!(parse_period_value("0.000082339").unwrap(), Duration::from_nanos(82339));
    parse_period_value("0.0000823391").unwrap_err();
    parse_period_value("0").unwrap_err();
    parse_period_value("0.0").unwrap_err();
    parse_period_value("-1").unwrap_err();
}