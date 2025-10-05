// SPDX-License-Identifier: GPL-2.0-or-later

use crate::error::ArgumentError;
use crate::arguments::lib::ComplexArgGroup;
use crate::event::EventType;
use crate::key::{Key, KeyParser};
use crate::stream::oscillator::Oscillator;
use crate::time::Duration;

/// Represents a --oscillate argument.
pub(super) struct OscillateArg {
    // Note: regardless of what `keys` says, only EV_KEY events will be oscillated.
    pub keys: Vec<Key>,

    pub active_time: Duration,
    pub inactive_time: Duration,
}

impl OscillateArg {
	pub fn parse(args: Vec<String>) -> Result<Self, ArgumentError> {
        let arg_group = ComplexArgGroup::parse(args,
            &[],
            &["period"],
            false,
            true,
        )?;

        // Accepts some keys that are not specifically EV_KEY, such as "@in", which is why the oscillator
        // will later do a manual check to make sure it only works with events that are EV_KEY and match
        // one of the provided keys.
        let keys = KeyParser {
            default_value: "",
            allow_values: false,
            allow_transitions: false,
            allow_ranges: false,
            allow_domains: true,
            allow_types: true,
            allow_relative_values: false,
            type_whitelist: Some(vec![EventType::KEY]),
            namespace: crate::event::Namespace::User,
        }
            .parse_all(&arg_group.get_keys_or_empty_key())?;

        let period_ns = super::delay::parse_period_as_nanoseconds(
            &arg_group.require_unique_clause("period")?
        )?;

        // The period is split over an active period and an inactive period, requiring a minimum of two
        // nanoseconds to make this split. (Which is not to say that any CPU can keep up with emitting
        // event every two nanoseconds, but this check just makes the program _theoretically_ sound.)
        if period_ns < 2 {
            return Err(ArgumentError::new("The period must be at least two nanoseconds."));
        }
        let active_time_ns = period_ns.div_ceil(2);
        let inactive_time_ns = period_ns - active_time_ns;

        Ok(Self {
            keys,
            active_time: Duration::from_nanos(active_time_ns),
            inactive_time: Duration::from_nanos(inactive_time_ns),
        })
    }

    pub fn compile(self) -> Oscillator {
        Oscillator::new(self.keys, self.active_time, self.inactive_time)
    }
}
