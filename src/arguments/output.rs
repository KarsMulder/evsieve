// SPDX-License-Identifier: GPL-2.0-or-later

use crate::predevice::RepeatMode;
use crate::error::ArgumentError;
use crate::arguments::lib::ComplexArgGroup;
use crate::key::{Key, KeyParser};
use crate::event::Namespace;
use std::path::PathBuf;

const DEFAULT_NAME: &str = "Evsieve Virtual Device";

/// Contains properties that evsieve itself does not care about, but are visible to other programs.
#[derive(Clone)]
pub struct DeviceProperties {
    pub name: String,
    pub device_id: Option<DeviceId>,
    pub version: Option<u16>,
    pub bus: Option<u16>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct DeviceId {
    pub vendor_id: u16,
    pub product_id: u16,
}

pub(super) struct OutputDevice {
    pub create_link: Option<PathBuf>,
    pub keys: Vec<Key>,
    pub repeat_mode: RepeatMode,
    pub properties: DeviceProperties,
}

impl OutputDevice {
	pub fn parse(args: Vec<String>) -> Result<OutputDevice, ArgumentError> {
        let arg_group = ComplexArgGroup::parse(args,
            &["repeat"],
            &["create-link", "repeat", "name", "device-id", "version", "bus"],
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

        // Parse special properties of the output device that shall be created.
        let name = arg_group.get_unique_clause("name")?.unwrap_or_else(|| DEFAULT_NAME.to_owned());
        if name.is_empty() {
            return Err(ArgumentError::new("Output device name cannot be empty."));
        }

        let device_id = match arg_group.get_unique_clause("device-id")? {
            Some(device_id_str) => Some(interpret_device_id(&device_id_str)?),
            None => None,
        };
        let version = match arg_group.get_unique_clause("version")? {
            Some(version_str) => Some(interpret_hex_clause("version", &version_str)?),
            None => None,
        };
        let bus = match arg_group.get_unique_clause("bus")? {
            Some(bus_str) => Some(interpret_hex_clause("bus", &bus_str)?),
            None => None,
        };

        // Parse the keys that shall be sent to this output device.
        let key_strs = arg_group.get_keys_or_empty_key();
        let mut keys = Vec::new();
        for &namespace in &[Namespace::User, Namespace::Yielded] {
            keys.append(
                &mut KeyParser::default_filter().with_namespace(namespace).parse_all(&key_strs)?
            );
        }

		Ok(OutputDevice {
            create_link: arg_group.get_unique_clause("create-link")?.map(PathBuf::from),
            keys, repeat_mode,
            properties: DeviceProperties {
                name, device_id, version, bus
            },
        })
    }
}

/// Tries to parse a clause like --bus=004a. The clause can contain up to four hexadecimal characters.
fn interpret_hex_clause(property_name: &str, value_str: &str) -> Result<u16, ArgumentError> {
    parse_hex(value_str).ok_or_else(|| ArgumentError::new(
        format!("Cannot interpret the {} value \"{}\" as a 16-bit hexadecimal string. Please use up to four characters from the set 0-9,a-f.", property_name, value_str)
    ))
}

/// Parses a hexadecimal u16 without proper error reporting.
fn parse_hex(value_str: &str) -> Option<u16> {
    // The Rust documentation says that the `u16::from_str_radix` allows the string to start
    // with a + sign, but allows for no other exceptions, so this should be enough.
    if value_str.starts_with('+') {
        return None;
    }
    u16::from_str_radix(value_str, 16).ok()
}

/// Tries to parse a vendor_id:product_id style device ID.
fn interpret_device_id(id_str: &str) -> Result<DeviceId, ArgumentError> {
    interpret_device_id_inner(id_str).ok_or_else(|| {
        ArgumentError::new(format!(
            "Cannot interpret \"{}\" as a device ID. Please provide it in the form vendor_id:product_id in hexadecimal format, for example \"--device-id=045e:082c\".", id_str
        ))
    })
}

/// Tries to parse a device id without proper error reporting.
fn interpret_device_id_inner(id_str: &str) -> Option<DeviceId> {
    let (vendor_id_str, product_id_str) = str::split_once(id_str, ':')?;
    let vendor_id = parse_hex(vendor_id_str)?;
    let product_id = parse_hex(product_id_str)?;
    Some(DeviceId { vendor_id, product_id })
}
