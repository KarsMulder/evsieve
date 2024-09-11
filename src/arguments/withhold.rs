// SPDX-License-Identifier: GPL-2.0-or-later

use crate::event::EventType;
use crate::error::ArgumentError;
use crate::arguments::lib::ComplexArgGroup;
use crate::arguments::hook::HookArg;
use crate::stream::hook::Trigger;
use crate::key::{Key, KeyParser};

// The --withhold argument imposes the following rules upon the preceding hooks:
// 1. None of the hooks may dispatch events (via send-key) that can match any of the preceding hooks.
// 2. Only events of type EV_KEY (key/btn) can be withheld.
// 3. All keys on the hook/withhold that may interact with withheld events must be pure.

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
        parser.type_whitelist = Some(vec![EventType::KEY]);
        let keys = parser.parse_all(&arg_group.get_keys_or_empty_key())?;

        Ok(WithholdArg { keys, associated_triggers: Vec::new() })
    }

    pub fn associate_hooks(&mut self, hooks: &mut [&mut HookArg]) -> Result<(), ArgumentError> {
        if hooks.is_empty() {
            return Err(ArgumentError::new("A --withhold argument must be preceded by at least one --hook argument."));
        }

        // Determine all keys that can be send from --hook send-key.
        let sendable_events: Vec<&Key> = hooks.iter().flat_map(|hook|
            hook.event_dispatcher.sendable_events()
        ).collect();

        // Verify that the constraints on the preceding hooks are upheld.
        for hook_arg in hooks.iter() {
            for (key, key_str) in &hook_arg.keys_and_str {
                // Make sure no hook can match on a key that can be sent from the same set.
                if sendable_events.iter().any(|send_key| send_key.intersects_with(key)) {
                    return Err(ArgumentError::new(format!(
                        "It is not possible to use --withhold on a set of hooks where any of the hooks has an input key that matches any event that can be dispatched by any of the hooks through means such as \"send-key=...\" or \"send-event=...\". The key \"{}\" violates this constraint.", key_str
                    )));
                }

                // If no events that match this trigger will ever be withheld, we do not need
                // to impose further restrictions on this trigger.
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
                let pedantic_parser = KeyParser::pure().and_filter(super::hook::PARSER);
                if pedantic_parser.parse(key_str).is_err() {
                    return Err(ArgumentError::new(format!(
                        "Cannot use --withhold after a --hook that activates on events with a specific value such as \"{}\".",
                        key_str
                    )));
                }
            }
        }

        self.associated_triggers.extend(
            hooks.iter_mut()
                 .map(|hook_arg| hook_arg.compile_trigger()
            )
        );

        Ok(())
    }
}
