// SPDX-License-Identifier: GPL-2.0-or-later

use crate::error::ArgumentError;
use crate::arguments::lib::ComplexArgGroup;
use crate::key::{Key, KeyParser};
use crate::stream::print::{EventPrinter, EventPrintMode};

/// Represents a --print argument.
pub(super) struct PrintArg {
    pub keys: Vec<Key>,
    pub mode: EventPrintMode,
}

impl PrintArg {
	pub fn parse(args: Vec<String>) -> Result<PrintArg, ArgumentError> {
        let arg_group = ComplexArgGroup::parse(args,
            &[],
            &["format"],
            false,
            true,
        )?;

        let keys = KeyParser::default_filter().parse_all(&arg_group.get_keys_or_empty_key())?;

        let mode = match arg_group.get_unique_clause("format")? {
            Some(value) => match value.as_str() {
                "direct" => EventPrintMode::Direct,
                "default" => EventPrintMode::Detailed,
                other => return Err(ArgumentError::new(format!("Invalid --print format: {}", other))),
            } ,
            None => EventPrintMode::Detailed,
        };

        Ok(PrintArg { keys, mode })
    }

    pub fn compile(self) -> EventPrinter {
        EventPrinter::new(self.keys, self.mode)
    }
}