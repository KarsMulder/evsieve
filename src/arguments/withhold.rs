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

        let mut parser = KeyParser::pure();
        parser.forbid_non_EV_KEY = true;
        let keys = parser.parse_all(&arg_group.get_keys_or_empty_key())?;

        Ok(WithholdArg { keys, associated_triggers: Vec::new() })
    }

    pub fn associate_hooks(&mut self, hooks: &[&HookArg]) -> Result<(), ArgumentError> {
        if hooks.is_empty() {
            return Err(ArgumentError::new("A --withhold argument must be preceded by at least one --hook argument."));
        }

        #[allow(non_snake_case)]
        // If the user has explicitly specified that this --withhold should only apply to
        // events of type EV_KEY, then we don't care what keys were added to the hooks.
        // If the user has not explicitly stated their wishes, we must verify that all hooks
        // only have keys of type EV_KEY.
        // TODO: this is fragile code that runs at risk of getting broken by adding an
        // KeyProperty::EventType(EventType::KEY) as default.
        let inherently_requires_EV_KEY: bool = self.keys.iter().all(
            |key| key.requires_event_type() == Some(EventType::KEY)
        );

        // Verify that the constrains on the preceding hooks are upheld.
        for hook_arg in hooks {
            for (key, key_str) in hook_arg.keys.iter().zip(&hook_arg.keys_str) {
                if !  inherently_requires_EV_KEY
                   && key.requires_event_type() != Some(EventType::KEY)
                {
                    return Err(ArgumentError::new(format!(
                        "Cannot use --withhold after a hook that triggers on the key \"{}\". Only events of type \"key\" or \"btn\" can be withheld. If you wish for this --withhold to ignore non-EV_KEY-type events, then you can get rid of this error by explicitly specifying \"--withhold key btn\".",
                        key_str,
                    )));
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
