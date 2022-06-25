// SPDX-License-Identifier: GPL-2.0-or-later

use crate::error::ArgumentError;
use crate::arguments::lib::ComplexArgGroup;

/// Represents a --merge argument.
pub(super) struct ControlFifoArg {
    pub paths: Vec<String>,
}

impl ControlFifoArg {
	pub fn parse(args: Vec<String>) -> Result<ControlFifoArg, ArgumentError> {
        let arg_group = ComplexArgGroup::parse(args,
            &[],
            &[],
            true,
            false,
        )?;

        Ok(ControlFifoArg {
            paths: arg_group.paths
        })
    }
}