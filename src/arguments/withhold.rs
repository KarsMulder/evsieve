// SPDX-License-Identifier: GPL-2.0-or-later

use crate::event::EventType;
use crate::error::ArgumentError;
use crate::arguments::lib::ComplexArgGroup;
use crate::arguments::hook::HookArg;
use crate::hook::Trigger;
use crate::key::{Key, KeyParser};
use crate::range::Range;

/// Represents a --withhold argument.
pub(super) struct WithholdArg {
    pub keys: Vec<Key>,
    /// All the triggers of all --hook arguments that come before a --withhold argument.
    pub associated_triggers: Vec<Trigger>,
}

impl WithholdArg {
	pub fn parse(args: Vec<String>) -> Result<WithholdArg, ArgumentError> {
        let arg_group = ComplexArgGroup::parse(args,
            &[],
            &[],
            false,
            true,
        )?;

        // TODO: require EV_KEY here.
        let keys = KeyParser::pure()
            .parse_all(&arg_group.get_keys_or_empty_key())?;

        Ok(WithholdArg { keys, associated_triggers: Vec::new() })
    }

    pub fn associate_hooks(&mut self, hooks: &[&HookArg]) -> Result<(), ArgumentError> {
        if hooks.is_empty() {
            return Err(ArgumentError::new("A --withhold argument must be preceded by at least one --hook argument."));
        }

        // Verify that the constrains on the preceding hooks are upheld.
        for hook_arg in hooks {
            for key in &hook_arg.hold_keys {
                // TODO: Ignore keys that have no intersection with self.key.
                // Only permit matching on events of type EV_KEY.
                if key.requires_event_type() != Some(EventType::KEY) {
                    // TODO: more helpful error
                    return Err(ArgumentError::new("Cannot use --withhold after a hook that triggers on event of types other than \"key\" or \"btn\"."));
                }
                // Only permit matching with default (unspecified) values.
                // TODO: forbid keys like "key:a:1~"" instead of a plain "key:a"
                const DEFAULT_RANGE: Range = Range::new(Some(1), None);
                match key.clone().pop_value() {
                    None | Some(DEFAULT_RANGE) => (),
                    Some(_) => {
                        // TODO: more helpful error.
                        return Err(ArgumentError::new("Cannot use --withhold after a hook that activates on events with a specified value."));
                    }
                }
            }
        }

        self.associated_triggers.extend(
            hooks.into_iter()
                 .map(|hook_arg| hook_arg.compile_trigger())
        );

        Ok(())
    }
}
