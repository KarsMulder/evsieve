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

