// SPDX-License-Identifier: GPL-2.0-or-later

// File format description.
// The file must start with the following two lines, followed up by a blank line:
//
// Evsieve event device capabilities description file
// Format version: 1.0
//
// The next line must be a "Path: " line followed up by the absolute path to the input device.
// This path must have been escaped as follows:
//
// Newline -> \n
// Backslash -> \\
//
// The path must be UTF-8. If it is not UTF-8 or contains a \ followed up by an invalid escape
// sequence, the file is invalid.
//
// Next must be a "Capabilities:", followed up by a newline, with each line thereafter containing
// a capability of the device. These capabilities are described as type:code for most capabilities.
// In the special case of EV_ABS type capabilities, they must be additional axis information specified
// in the following way:
//
// abs:CODE (min=INT, max=INT, fuzz=INT, flat=INT, resolution=INT)
//
// EV_REP capabilities must be encoded as follows:
//
// rep (delay=UINT16, period=UINT16)
// 
// All the above fields must be provided in the exact order as specified above. The file must end
// with a newline (\n) character. An example valid file is shown below:
//
//
// ```
// Evsieve event device capabilities description file
// Format version: 1.0
//
// Path: /dev/input/by-id/my\nescaped\\path
//
// Capabilities:
// key:a
// key:b
// key:c
// abs:x (min=-5, max=5, fuzz=2, flat=1, resolution=TODO)
// rep (delay=UINT16, period=U16)
// ```

