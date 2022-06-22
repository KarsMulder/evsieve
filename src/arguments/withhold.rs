// SPDX-License-Identifier: GPL-2.0-or-later

use crate::event::EventType;
use crate::error::ArgumentError;
use crate::arguments::lib::ComplexArgGroup;
use crate::arguments::hook::HookArg;
use crate::stream::hook::Trigger;
use crate::key::{Key, KeyParser};

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

    pub fn associate_hooks(&mut self, hooks: &mut [&mut HookArg]) -> Result<(), ArgumentError> {
        if hooks.is_empty() {
            return Err(ArgumentError::new("A --withhold argument must be preceded by at least one --hook argument."));
        }

        // Verify that the constraints on the preceding hooks are upheld.
        for hook_arg in hooks.iter_mut() {
            for (key, key_str) in &hook_arg.keys_and_str {
                // If no events that match this trigger will ever be withheld, we do not need
                // to impose restrictions on this trigger.
                if ! self.keys.iter().any(|self_key| self_key.intersects_with(key)) {
                    continue;
                }

                // Make sure that all triggers whose associated may possibly be withheld can
                // only trigger on events of type EV_KEY.
                match key.requires_event_type() {
                    Some(EventType::KEY) => (),
                    None => return Err(ArgumentError::new(format!(
                        "Cannot use --withhold after a hook that triggers on the key \"{}\", because this key can be triggered by events of any event type.",
                        key_str,
                    ))),
                    Some(_) => return Err(ArgumentError::new(format!(
                        "Cannot use --withhold after a hook that triggers on the key \"{}\". Only events of type \"key\" or \"btn\" can be withheld. If you wish for this --withhold to ignore non-EV_KEY-type events, then you can get rid of this error by explicitly specifying \"--withhold key btn\".",
                        key_str,
                    ))),
                }

                // Only permit matching with default (unspecified) values.
                let mut pedantic_parser = super::hook::PARSER;
                pedantic_parser.allow_values = false;
                if pedantic_parser.parse(key_str).is_err() {
                    return Err(ArgumentError::new(format!(
                        "Cannot use --withhold after a --hook that activates on events with a specific value such as \"{}\".",
                        key_str
                    )));
                }
            }
        }

        // Inform all associated hooks to mark events as withholdable.
        for hook in hooks.iter_mut() {
            hook.mark_withholdable = true;
        }

        self.associated_triggers.extend(
            hooks.iter_mut()
                 .map(|hook_arg| hook_arg.compile_trigger())
        );

        Ok(())
    }
}
