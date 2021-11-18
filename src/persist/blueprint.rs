// SPDX-License-Identifier: GPL-2.0-or-later

use crate::activity::ActivityLink;
use crate::io::input::{InputDevice, InputDeviceName};
use crate::predevice::PreInputDevice;
use crate::capability::Capabilities;
use crate::error::{SystemError};

/// Represents something can can be used to re-open a closed input device.
pub struct Blueprint {
    pub pre_device: PreInputDevice,
    pub capabilities: Capabilities,
    pub name: InputDeviceName,

    /// Prevents evsieve from automatically exiting as long as a viable Blueprint remains.
    pub activity_link: ActivityLink,
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

        // Do sanity checks so we don't accidentally re-open the wrong device.
        if input_device.name() != &self.name {
            return Err(SystemError::new(format!(
                "Cannot reopen input device \"{}\": the reattached device's name differs from the original name. Original: {}, new: {}",
                self.pre_device.path.display(),
                self.name.to_string_lossy(), input_device.name().to_string_lossy()
            )))
        }
        if *input_device.capabilities() != self.capabilities {
            return Err(SystemError::new(format!(
                "Cannot reopen input device \"{}\": the reattached device's capabilities are different from the original device that disconnected.",
                self.pre_device.path.display()
            )));
        }
        
        Ok(Some(input_device))
    }
}