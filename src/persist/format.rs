// SPDX-License-Identifier: GPL-2.0-or-later

// Global overview:
// 1. Magic number.
// 2. File length.
// 3. Event type count.
// 4. Event code blocks.
// 5. Special blocks.
//
// # Magic number
// The file should start with the following eight bytes as magic number:
//      45 56 53 56 41 e7 75 01
// The first four bytes are "EVSV" in ASCII, the next three bytes were taken from a random number generator, and the
// last byte represents the version number of this file format.
//
// # File length
// The next four bytes shall be an u32 representing the total length of the file in bytes.
//
// # Event type count
// It should then follow up with an u16 representing the amount of event types that are supported by this device.
// This u16, like every other integer, shall be encoded in little endian format. We call this `num_types`.
//
// # Event code blocks
// Next, for each supported type, a type block appears. This type block has the following format:
// 1. First, an u16 appears telling you the numeric value of this type such as `EV_KEY`.
// 2. Second, an u16 appears that tells you how many event codes are supported for this type. We call this `num_codes`.
// 3. Thereafter, a `num_codes` amount of u16s follow, each representing a supported event code such as `KEY_A`.
//    These codes must be sorted in ascending order.
//
// # Special blocks
// After all type blocks have been processed, special blocks may follow. A special block must follow if EV_ABS or EV_REP
// event types were among the supported event types. Each special block starts with a magic u16. The special blocks must
// appear in ascending order of that magic number.
//
// The special block for EV_ABS events has the following structure:
// 1. First, the magic u16 of value `1` appears (in bytes: 01 00)
// 2. Then, for each supported event code, five i32 values follow, representing the following values:
//        abs_min, abs_max, flat, fuzz, resolution
//    These appear in the same order as the codes appeared in the event code block for the EV_ABS event type.
//    The i32 shall be encoded in low-endian using two's complement.
// 
// The special block for EV_REP events has the following structure:
// 1. First, the magic u16 of value `2` appears (in bytes: 02 00)
// 2. Then, two i32s for the following two values appear: `rep_delay`, `rep_period`.
// These two i32s must appear even in the unlikely case that either REP_DELAY or REP_PERIOD was not supported by the
// original device. They may take arbitrary values in that case.
//
// The last special block, which must always appear, contains a header of the bytes "ff ff" and has no body.

use std::collections::{HashMap, HashSet};
use std::convert::TryInto;
use std::fmt::Debug;
use std::io::{BufRead, Cursor};

use crate::capability::{Capabilities, RepeatInfo, AbsInfo, AbsMeta};
use crate::ecodes;
use crate::event::{EventType, EventCode};
use crate::error::{RuntimeError, InternalError};

// The magic header that every file starts with.
const MAGIC_NUMBER: [u8; 8] = [0x45, 0x56, 0x53, 0x56, 0x41, 0xe7, 0x75, 01];
const NUM_FILE_LEN_BYTES: usize = std::mem::size_of::<u32>();

// Magic number to indentify special blocks.
const EV_ABS_BLOCK_NUMBER: u16 = 0x0001;
const EV_REP_BLOCK_NUMBER: u16 = 0x0002;
const FINAL_BLOCK_NUMBER: u16  = 0xffff;

// Tells you that a file could not be read because its format was different from what was expected.
pub struct InvalidFormatError;
impl Debug for InvalidFormatError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Invalid file format")
    }
}

pub fn encode(caps: &Capabilities) -> Result<Vec<u8>, RuntimeError> {
    let body = encode_body(&caps)?;

    // 1. Magic number
    let mut header: Vec<u8> = Vec::new();
    header.extend(MAGIC_NUMBER);

    // 2. File length
    let file_length_usize = header.len() + NUM_FILE_LEN_BYTES + body.len();
    let file_length_u32: u32 = file_length_usize.try_into()
        .map_err(|_| InternalError::new("Total file size exceeds 4GB. Too large."))?;
    push_u32(&mut header, file_length_u32);

    // Concatenate the header and the body.
    let mut result = header;
    result.extend_from_slice(&body);
    if result.len() != file_length_usize {
        return Err(InternalError::new("Generated file length differs from expected size. This is a bug.").into());
    }

    if cfg!(debug_assertions) {
        let decoded_caps = decode(&result).expect("Failed to decode the generated file.");
        assert!(caps.is_compatible_with(&decoded_caps));
    }
    
    Ok(result)
}

pub fn decode(source: &[u8]) -> Result<Capabilities, InvalidFormatError> {
    let source_length = source.len();

    // 1. Verify magic number
    if source[0 .. MAGIC_NUMBER.len()] != MAGIC_NUMBER {
        return Err(InvalidFormatError);
    }
    let mut reader = Cursor::new(source);
    reader.set_position(MAGIC_NUMBER.len() as u64);

    // 2. Verify file length
    let declared_length = read_u32(&mut reader)?;
    if declared_length as usize != source_length {
        return Err(invalid_format());
    }

    decode_body(&mut reader)
}

/// The body represents the whole file except for the magic number and the file length.
fn encode_body(caps: &Capabilities) -> Result<Vec<u8>, InternalError> {
    let mut body: Vec<u8> = Vec::new();

    // 3. Event type count
    let mut event_types: Vec<EventType> = caps.ev_types().into_iter().collect();
    event_types.sort_by_key(|&ev_type| u16::from(ev_type));
    let num_types: u16 = event_types.len().try_into()
        .map_err(|_| InternalError::new("Too many event types to fit in an u16."))?;
    push_u16(&mut body, num_types);

    // 4. Event code blocks
    for &ev_type in &event_types {
        encode_event_block(&mut body, caps, ev_type)?;
    }

    // 5. Special blocks
    if event_types.contains(&EventType::ABS) {
        encode_special_abs_block(&mut body, caps)?;
    }
    if event_types.contains(&EventType::REP) {
        encode_special_rep_block(&mut body, caps);
    }
    push_u16(&mut body, FINAL_BLOCK_NUMBER);

    Ok(body)
}

fn decode_body(source: &mut impl BufRead) -> Result<Capabilities, InvalidFormatError> {
    // 3. Event type count
    let num_types = read_u16(source)?;

    // 4. Event code blocks
    let mut type_codes_map: HashMap<EventType, Vec<EventCode>> = HashMap::new();
    for _ in 0 .. num_types {
        let (ev_type, codes) = decode_event_block(source)?;
        if type_codes_map.contains_key(&ev_type) {
            return Err(invalid_format());
        }
        type_codes_map.insert(ev_type, codes);
    }

    // 5. Special blocks
    let abs_info = if let Some(abs_codes) = type_codes_map.get(&EventType::ABS) {
        decode_special_abs_block(source, abs_codes)?
    } else {
        HashMap::new()
    };
    let rep_info = if type_codes_map.contains_key(&EventType::REP) {
        Some(decode_special_rep_block(source)?)
    } else {
        None
    };
    expect_u16(source, FINAL_BLOCK_NUMBER)?;

    let codes: HashSet<EventCode> = type_codes_map.into_iter().flat_map(|(_type, codes)| codes).collect();
    Ok(Capabilities {
        codes,
        abs_info,
        rep_info,
    })
}

fn encode_event_block(buffer: &mut Vec<u8>, caps: &Capabilities, ev_type: EventType) -> Result<(), InternalError> {
    let event_codes = sorted_event_codes_for_type(caps, ev_type);
    let num_event_codes: u16 = event_codes.len().try_into()
        .map_err(|_| InternalError::new(format!("Too many event codes of type %{} to fit in an u16.", u16::from(ev_type))))?;

    push_u16(buffer, u16::from(ev_type));
    push_u16(buffer, num_event_codes);
    for event_code in event_codes {
        push_u16(buffer, event_code.code());
    }

    Ok(())
}

fn decode_event_block(source: &mut impl BufRead) -> Result<(EventType, Vec<EventCode>), InvalidFormatError> {
    let ev_type_u16 = read_u16(source)?;
    if ev_type_u16 > ecodes::EV_MAX {
        return Err(invalid_format());
    }
    let ev_type = EventType::new(ev_type_u16);
    let max_code = ecodes::event_type_get_max(ev_type).unwrap_or(u16::MAX);

    let num_event_codes = read_u16(source)?;
    let mut event_codes = Vec::with_capacity(num_event_codes.into());

    for _ in 0 .. num_event_codes {
        let event_code_u16 = read_u16(source)?;
        if event_code_u16 > max_code {
            return Err(invalid_format());
        }
        let event_code = EventCode::new(ev_type, event_code_u16);
        event_codes.push(event_code);
    }

    Ok((ev_type, event_codes))
}

fn encode_special_abs_block(buffer: &mut Vec<u8>, caps: &Capabilities) -> Result<(), InternalError> {
    push_u16(buffer, EV_ABS_BLOCK_NUMBER);
    let abs_codes = sorted_event_codes_for_type(caps, EventType::ABS);
    for abs_code in abs_codes {
        let Some(abs_info) = caps.abs_info.get(&abs_code) else {
            return Err(InternalError::new(format!(
                "The capabilities contain the abs-type event {}, but do not contain data about its axes. This is a bug.",
                crate::ecodes::event_name(abs_code)
            )).into());
        };
        if abs_info.min_value > abs_info.max_value {
            return Err(InternalError::new(format!(
                "The absolute axes {} has a minimum value larger than its maximum value.",
                crate::ecodes::event_name(abs_code)
            )).into());
        }

        push_i32(buffer, abs_info.min_value);
        push_i32(buffer, abs_info.max_value);
        push_i32(buffer, abs_info.meta.flat);
        push_i32(buffer, abs_info.meta.fuzz);
        push_i32(buffer, abs_info.meta.resolution);
    }

    Ok(())
}

fn decode_special_abs_block(source: &mut impl BufRead, abs_codes: &[EventCode]) -> Result<HashMap<EventCode, AbsInfo>, InvalidFormatError> {
    expect_u16(source, EV_ABS_BLOCK_NUMBER)?;
    let mut abs_info: HashMap<EventCode, AbsInfo> = HashMap::new();
    for &abs_code in abs_codes {
        let min_value  = read_i32(source)?;
        let max_value  = read_i32(source)?;
        let flat       = read_i32(source)?;
        let fuzz       = read_i32(source)?;
        let resolution = read_i32(source)?;

        if min_value > max_value {
            return Err(invalid_format());
        }
        let value = (((min_value as i64) + (max_value as i64)) / 2) as i32;

        abs_info.insert(abs_code, {
            AbsInfo {
                min_value, max_value,
                meta: AbsMeta {
                    fuzz, flat, resolution, value,
                },
            }
        });
    }

    Ok(abs_info)
}

fn encode_special_rep_block(buffer: &mut Vec<u8>, caps: &Capabilities) {
    push_u16(buffer, EV_REP_BLOCK_NUMBER);
    let rep_info = caps.rep_info.unwrap_or(RepeatInfo::kernel_default());
    push_i32(buffer, rep_info.delay);
    push_i32(buffer, rep_info.period);
}

fn decode_special_rep_block(source: &mut impl BufRead) -> Result<RepeatInfo, InvalidFormatError> {
    expect_u16(source, EV_REP_BLOCK_NUMBER)?;
    let delay = read_i32(source)?;
    let period = read_i32(source)?;
    Ok(RepeatInfo { delay, period })
}

// Handy functions for writing numbers to a vector of bytes. Uses low-endian encoding.
fn push_u16(buffer: &mut Vec<u8>, value: u16) {
    buffer.extend(value.to_le_bytes());
}
fn push_i32(buffer: &mut Vec<u8>, value: i32) {
    buffer.extend(value.to_le_bytes());
}
fn push_u32(buffer: &mut Vec<u8>, value: u32) {
    buffer.extend(value.to_le_bytes());
}

// Handy functions for reading numbers. Unfortunately I can't make these generic since from_le_bytes is not
// associated with any trait.
fn read_u16(source: &mut impl BufRead) -> Result<u16, InvalidFormatError> {
    let mut buffer: [u8; std::mem::size_of::<u16>()] = Default::default();
    source.read_exact(buffer.as_mut_slice()).map_err(|_| invalid_format())?;
    Ok(u16::from_le_bytes(buffer))
}
fn read_i32(source: &mut impl BufRead) -> Result<i32, InvalidFormatError> {
    let mut buffer: [u8; std::mem::size_of::<i32>()] = Default::default();
    source.read_exact(buffer.as_mut_slice()).map_err(|_| invalid_format())?;
    Ok(i32::from_le_bytes(buffer))
}
fn read_u32(source: &mut impl BufRead) -> Result<u32, InvalidFormatError> {
    let mut buffer: [u8; std::mem::size_of::<u32>()] = Default::default();
    source.read_exact(buffer.as_mut_slice()).map_err(|_| invalid_format())?;
    Ok(u32::from_le_bytes(buffer))
}

/// Returns an error if the next bytes are not equal to the expected value
fn expect_u16(source: &mut impl BufRead, expected_value: u16) -> Result<(), InvalidFormatError> {
    let found_value = read_u16(source)?;
    if found_value != expected_value {
        return Err(invalid_format());
    }
    Ok(())
}

/// Returns an error that tells you that the format of the read file was not what was expected.
fn invalid_format() -> InvalidFormatError {
    InvalidFormatError
}

/// Returns all event codes of a specific event type within the provided capabilities as a sorted vector. This function
/// is deterministic: calling it twice with the same capabilities must return the same vector.
fn sorted_event_codes_for_type(caps: &Capabilities, ev_type: EventType) -> Vec<EventCode> {
    let mut codes: Vec<EventCode> = caps.codes.iter()
        .filter(|code| code.ev_type() == ev_type)
        .copied().collect();
    codes.sort();

    codes
}
