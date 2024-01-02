// SPDX-License-Identifier: GPL-2.0-or-later

// Global overview:
// 1. Magic number.
// 2. File length.
// 3. Event type count.
// 4. Event code blocks.
// 5. Special blocks.
// 6. CRC32 Hash.
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
//
// # CRC32 Hash
// Finally, the last four bytes are a CRC32 hash of the entire file that came before.

use std::convert::TryInto;

use crate::capability::{Capabilities, RepeatInfo};
use crate::event::{EventType, EventCode};
use crate::error::{RuntimeError, InternalError};

// The magic header that every file starts with.
const MAGIC_NUMBER: [u8; 8] = [0x45, 0x56, 0x53, 0x56, 0x41, 0xe7, 0x75, 01];

// Magic number to indentify special blocks.
const EV_ABS_BLOCK_NUMBER: u16 = 0x0001;
const EV_REP_BLOCK_NUMBER: u16 = 0x0002;
const FINAL_BLOCK_NUMBER: u16  = 0xffff;

pub fn encode(caps: &Capabilities) -> Result<Vec<u8>, RuntimeError> {
    let body = encode_body(&caps)?;

    // Magic number
    let mut header: Vec<u8> = Vec::new();
    header.extend(MAGIC_NUMBER);

    // File length
    const NUM_FILE_LEN_BYTES: usize = std::mem::size_of::<u32>();
    assert!(NUM_FILE_LEN_BYTES == 4);
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

    Ok(result)
}

/// The body represents the whole file except for the magic number and the file length.
fn encode_body(caps: &Capabilities) -> Result<Vec<u8>, InternalError> {
    let mut body: Vec<u8> = Vec::new();

    // Event type count
    let mut event_types: Vec<EventType> = caps.ev_types().into_iter().collect();
    event_types.sort_by_key(|&ev_type| u16::from(ev_type));
    let num_types: u16 = event_types.len().try_into()
        .map_err(|_| InternalError::new("Too many event types to fit in an u16."))?;
    push_u16(&mut body, num_types);

    // Event code blocks
    for &ev_type in &event_types {
        let event_codes = sorted_event_codes_for_type(caps, ev_type);
        let num_event_codes: u16 = event_codes.len().try_into()
            .map_err(|_| InternalError::new(format!("Too many event codes of type %{} to fit in an u16.", u16::from(ev_type))))?;

        push_u16(&mut body, u16::from(ev_type));
        push_u16(&mut body, num_event_codes);
        for event_code in event_codes {
            push_u16(&mut body, event_code.code());
        }
    }

    // Special blocks
    if event_types.contains(&EventType::ABS) {
        push_u16(&mut body, EV_ABS_BLOCK_NUMBER);
        let abs_codes = sorted_event_codes_for_type(caps, EventType::ABS);
        for abs_code in abs_codes {
            let Some(abs_info) = caps.abs_info.get(&abs_code) else {
                return Err(InternalError::new(format!(
                    "The capabilities contain the abs-type event {}, but do not contain data about its axes. This is a bug.",
                    crate::ecodes::event_name(abs_code)
                )).into());
            };
            push_i32(&mut body, abs_info.min_value);
            push_i32(&mut body, abs_info.max_value);
            push_i32(&mut body, abs_info.meta.flat);
            push_i32(&mut body, abs_info.meta.fuzz);
            push_i32(&mut body, abs_info.meta.resolution);
        }
    }

    if event_types.contains(&EventType::REP) {
        push_u16(&mut body, EV_REP_BLOCK_NUMBER);
        let rep_info = caps.rep_info.unwrap_or(RepeatInfo::kernel_default());
        push_i32(&mut body, rep_info.delay);
        push_i32(&mut body, rep_info.period);
    }

    push_u16(&mut body, FINAL_BLOCK_NUMBER);

    Ok(body)
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

/// Returns all event codes of a specific event type within the provided capabilities as a sorted vector. This function
/// is deterministic: calling it twice with the same capabilities must return the same vector.
fn sorted_event_codes_for_type(caps: &Capabilities, ev_type: EventType) -> Vec<EventCode> {
    let mut codes: Vec<EventCode> = caps.codes.iter()
        .filter(|code| code.ev_type() == ev_type)
        .copied().collect();
    codes.sort();

    codes
}
