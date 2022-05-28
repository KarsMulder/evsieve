// SPDX-License-Identifier: GPL-2.0-or-later

// File format description.
//
// The is a binary file format. The file must start with the following six bytes as magic
// number to identify this file as an evsieve file:
//
// bf a5 7e 5d 02 e6
//
// Next follows an u16 which identifies the file version. The file version should be 1.
// If a higher number is read here, then the file is created by a future version of
// evsieve. This number, and all other numbers, must be encoded in low-endian format.
// Therefore, the next two bytes are:
//
// 01 00
//
// The next variable amount of  bytes represent the path of the device whose capabilities
// this file represents. The path MUST start with a / and MUST end with a null-byte. The
// length of the path is decided by finding the null byte. The path has no particular encoding,
// just like Linux paths don't have any encoding either.
//
// After the path's null byte, between 0 and 1 additional padding null bytes follow, until
// the current byte index is a multiple of 2.
//
// Then, an u16 integer $n$ follows, representing how many different types of events this device
// accepts. Thereafter, $n$ blocks follow representing the capabilities for each specific type,
// which has one of the following formats. All blocks must start with an u16 representing the
// event type; based on the event type, the block format is decided.
//
// A. Encoding generic capabilities.
//
// This block starts with an u16 representing the event type, followed up by an u16 $x$
// representing the amount of event codes with this type that are supported. This is followed
// up by $x$ u16's, each representing the code that this device supports. After those $2x$
// bytes, the next block starts.
//
// B. Encoding EV_ABS capabilities.
//
// TODO.
//
// C. Encoding EV_REP capabilities.
//
// TODO.

use std::path::Path;
use std::os::unix::ffi::OsStrExt;
use std::convert::TryInto;
use crate::event::{EventType, EventCode};
use crate::capability::Capabilities;
use crate::error::*;

const MAGIC_HEADER: [u8; 6] = [0xbf, 0xa5, 0x7e, 0x5d, 0x02, 0xe6];
const FORMAT_VERSION: u16 = 1;

pub fn encode(path: &Path, capabilities: &Capabilities) -> Result<(), RuntimeError> {
    let mut result: Vec<u8> = Vec::new();
    result.extend(&MAGIC_HEADER);
    write_u16(&mut result, FORMAT_VERSION);
    let path_bytes = path.as_os_str().as_bytes();

    // Enforce invariants on paths.
    const ASCII_SLASH_CODE: u8 = 0x2f;
    const NULL_BYTE: u8 = 0;
    if path_bytes[0] != ASCII_SLASH_CODE {
        return Err(InternalError::new(format!(
            "Cannot encode the path {}: path does not start with a /.", path.to_string_lossy()
        )).into());
    }

    if path_bytes.contains(&NULL_BYTE) {
        return Err(InternalError::new(format!(
            "Cannot encode the path {}: path contains a null byte.", path.to_string_lossy()
        )).into());
    }

    // Add the path, the null terminator, and the eventual padding byte.
    result.extend(path_bytes);
    write_u8(&mut result, NULL_BYTE);
    if result.len() %2 == 1 {
        write_u8(&mut result, NULL_BYTE);
    }

    let supported_types = capabilities.ev_types();
    for ev_type in supported_types {
        match ev_type {
            _ => encode_generic_event_type(&mut result, ev_type, &capabilities)?,
        }
    }


    Ok(())
}

fn encode_generic_event_type(
        result: &mut Vec<u8>, ev_type: EventType, capabilities: &Capabilities
) -> Result<(), RuntimeError> {
    // Get the supported codes as a vector of u16.
    let mut supported_codes: Vec<u16> = capabilities.codes
        .iter().filter(|code| code.ev_type() == ev_type)
        .map(|code| code.code())
        .collect();

    // Not strictly necessary, but helps ensuring that encoding the same device results
    // into the same file, instead of always having the codes in random order.
    supported_codes.sort_unstable();
    supported_codes.dedup();
    
    let num_supported_codes: u16 = match supported_codes.len().try_into() {
        Ok(value) => value,
        Err(_) => {
            // TODO: More helpful error message? Or choose u32 instead?
            return Err(InternalError::new(format!(
                "Cannot encode event type: too many event codes.",
            )).into());
        },
    };

    write_u16(result, num_supported_codes);
    for code in supported_codes {
        write_u16(result, code);
    }

    Ok(())
}

fn write_u16(buffer: &mut Vec<u8>, value: u16) {
    buffer.extend(value.to_le_bytes());
}
fn write_u8(buffer: &mut Vec<u8>, value: u8) {
    buffer.extend(value.to_le_bytes());
}
