// SPDX-License-Identifier: GPL-2.0-or-later

use crate::io::input::{InputDevice, InputDeviceName};
use crate::predevice::PreInputDevice;
use crate::capability::Capabilities;
use crate::error::{SystemError};

/// Represents something can can be used to re-open a closed input device.
pub struct Blueprint {
    pub pre_device: PreInputDevice,
    pub capabilities: Capabilities,
    pub name: InputDeviceName,
}

impl Blueprint {
    /// Tries to reopen the device from which this blueprint was generated.
    /// On success, returns the device. On failure, returns Ok(None). In case of a grave
    /// error that signals reopening should not be retried, returns Err(SystemError).
    pub fn try_open(&self) -> Result<Option<InputDevice>, SystemError> {
        if ! self.pre_device.path.exists() {
            return Ok(None);
        }
        let input_device = InputDevice::open(self.pre_device.clone())?;

        // Do sanity checks.
        if input_device.name() != &self.name {
            println!(
                "Warning: the reconnected device \"{}\" has a different name than expected. Expected name: \"{}\", new name: \"{}\".",
                self.pre_device.path.display(),
                self.name.to_string_lossy(),
                input_device.name().to_string_lossy(),
            );
        }

        // TODO: this may print warnings on capabilities differing only in value.`
        if *input_device.capabilities() != self.capabilities {
            println!(
                "Warning: the capabilities of the reconnected device \"{}\" are different than expected.",
                self.pre_device.path.display()
            );
        }
        
        Ok(Some(input_device))
    }
}