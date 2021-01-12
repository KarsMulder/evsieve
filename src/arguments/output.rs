// SPDX-License-Identifier: GPL-2.0-or-later

use crate::predevice::RepeatMode;
use crate::error::ArgumentError;
use crate::arguments::lib::ComplexArgGroup;
use crate::key::{Key, KeyParser};
use crate::event::Namespace;


pub(super) struct OutputDevice {
    pub create_link: Option<String>,
    pub keys: Vec<Key>,
    pub repeat_mode: RepeatMode,
}

impl OutputDevice {
	pub fn parse(args: Vec<String>) -> Result<OutputDevice, ArgumentError> {
        let arg_group = ComplexArgGroup::parse(args,
            &["repeat"],
            &["create-link", "repeat"],
            false,
            true,
        )?;

        let repeat_mode = match arg_group.get_unique_clause_or_default_if_flag("repeat", "enable")? {
            None => RepeatMode::Passive,
            Some(mode) => match mode.as_str() {
                "enable" => RepeatMode::Enable,
                "disable" => RepeatMode::Disable,
                "passive" => RepeatMode::Passive,
                _ => return Err(ArgumentError::new(format!("Invalid repeat mode \"{}\".", mode)))
            },
        };

        // Parse the keys that shall be sent to this output device.
        let key_strs = arg_group.get_keys_or_empty_key();
        let mut keys = Vec::new();
        for &namespace in &[Namespace::User, Namespace::Yielded] {
            keys.append(
                &mut KeyParser {
                    allow_ranges: true,
                    allow_transitions: true,
                    default_value: "",
                    namespace,
                }.parse_all(&key_strs)?
            );
        }

		Ok(OutputDevice {
            create_link: arg_group.get_unique_clause("create-link")?,
            keys, repeat_mode,
        })
    }
}