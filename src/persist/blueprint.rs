// SPDX-License-Identifier: GPL-2.0-or-later

use crate::io::input::{InputDevice, InputDeviceName};
use crate::predevice::PreInputDevice;
use crate::capability::Capabilities;
use crate::error::SystemError;

/// Represents something can can be used to re-open a closed input device.
pub struct Blueprint {
    pub pre_device: PreInputDevice,
    pub capabilities: Capabilities,
    /// The name that the input device is expected to have. None is unknown. It is not considered an error
    /// if the input device ends up having a different name than specified here, it is just cause to issue
    /// a warning to help find problematic setups.
    pub name: Option<InputDeviceName>,
}

pub enum TryOpenBlueprintResult {
    /// Represents that the blueprint was successfully opened.
    Success(InputDevice),
    /// Represents that the blueprint could not be opened right now, but you may try again later.
    NotOpened(Blueprint),
    /// Represents an error of sufficient magnitude that you should not try opening this blueprint again.
    Error(Blueprint, SystemError),
}

impl Blueprint {
    /// Tries to reopen the device from which this blueprint was generated.
    pub fn try_open(self) -> TryOpenBlueprintResult {
        if ! self.pre_device.path.exists() {
            if cfg!(feature = "debug-persistence") {
                println!("The path {} does not exist.", self.pre_device.path.to_string_lossy());
            }
            return TryOpenBlueprintResult::NotOpened(self);
        }
        let input_device = match InputDevice::open(self.pre_device) {
            Ok(device) => device,
            Err((pre_device, error)) => {
                return TryOpenBlueprintResult::Error(Blueprint {
                    pre_device,
                    capabilities: self.capabilities,
                    name: self.name,
                }, error);
            },
        };

        // Do sanity checks.
        if let Some(name) = self.name {
            if input_device.name() != &name {
                println!(
                    "Warning: the reconnected device \"{}\" has a different name than expected. Expected name: \"{}\", new name: \"{}\".",
                    input_device.path().display(),
                    name.to_string_lossy(),
                    input_device.name().to_string_lossy(),
                );
            }
        }

        // TODO: LOW-PRIORITY this may print warnings on capabilities differing only in value.
        if *input_device.capabilities() != self.capabilities {
            println!(
                "Warning: the capabilities of the reconnected device \"{}\" are different than expected.",
                input_device.path().display()
            );
        }
        
        TryOpenBlueprintResult::Success(input_device)
    }
}