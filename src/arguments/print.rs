// SPDX-License-Identifier: GPL-2.0-or-later

use crate::error::{ArgumentError, RuntimeError};
use crate::arguments::lib::ComplexArgGroup;
use crate::key::{Key, KeyParser};
use crate::event::Namespace;
use crate::print::{EventPrinter, EventPrintMode};

/// Represents a --print argument.
pub(super) struct PrintArg {
    pub keys: Vec<Key>,
    pub mode: EventPrintMode,
}

impl PrintArg {
	pub fn parse(args: Vec<String>) -> Result<PrintArg, RuntimeError> {
        let arg_group = ComplexArgGroup::parse(args,
            &[],
            &["format"],
            false,
            true,
        )?;

        let keys = KeyParser {
            allow_ranges: true,
            allow_transitions: true,
            default_value: "",
            namespace: Namespace::User,
        }.parse_all(&arg_group.get_keys_or_empty_key())?;

        let mode = match arg_group.get_unique_clause("format")? {
            Some(value) => match value.as_str() {
                "direct" => EventPrintMode::Direct,
                "default" => EventPrintMode::Detailed,
                other => return Err(ArgumentError::new(format!("Invalid --print format: {}", other)).into()),
            } ,
            None => EventPrintMode::Detailed,
        };

        Ok(PrintArg { keys, mode })
    }

    pub fn compile(self) -> EventPrinter {
        EventPrinter::new(self.keys, self.mode)
    }
}