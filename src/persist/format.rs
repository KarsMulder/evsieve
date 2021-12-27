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
// abs:x (min=-5, max=5, fuzz=2, flat=1, resolution=1)
// rep (delay=250, period=33)
// ```

use std::fmt::Write;

struct ParseError {
    message: String
}

impl ParseError {
    pub fn new(message: String) -> ParseError {
        ParseError { message }
    }
}

/// This and its twin `unescape_path()` are for escaping the path to input files so they never contain
/// newlines and a newline can be reliably interpreted as the end of the path.
fn escape_path(path: String) -> String {
    path.replace("\\", "\\\\")
        .replace("\n", "\\n")
}

fn unescape_path(path: String) -> Result<String, ParseError> {
    let mut iter = path.chars();
    let mut result = String::new();
    while let Some(character) = iter.next() {
        match character {
            '\\' => match iter.next() {
                Some('\\') => result.push('\\'),
                Some('n') => result.push('\n'),
                Some(other) => return Err(ParseError::new(format!(
                    "Invalid escape sequence: \\{}", other
                ))),
                None => return Err(ParseError::new("Backslash encountered at end of line.".to_owned())),
            },
            other => result.push(other),
        }
    }
    Ok(result)
}

fn format_device_path(path: String) -> String {
    let mut result = "Path: ".to_owned();
    result.push_str(&escape_path(path));
    result
}

fn parse_device_path(path_line: String) -> Result<String, ParseError> {
    let path = match path_line.strip_prefix("Path: ") {
        Some(path_escaped) => unescape_path(path_escaped.to_string())?,
        None => return Err(ParseError::new(format!(
            "Expected \"Path: something\", encountered: \"{}\"", path_line
        ))),
    };

    if path.starts_with('/') {
        Ok(path)
    } else {
        Err(ParseError::new(format!(
            "The path \"{}\" must be in absolute form.", path
        )))
    }
}

const MAGICAL_NUMBER_HEADER: &str = "Evsieve event device capabilities description file";
const FORMAT_VERSION_HEADER: &str = "Format version: 1.0";
const EMPTY_LINE: &str = "";

fn write() -> Result<String, std::fmt::Error> {
    let mut output: String = String::new();
    writeln!(output, "{}", MAGICAL_NUMBER_HEADER)?;
    writeln!(output, "{}", FORMAT_VERSION_HEADER)?;
    writeln!(output, "{}", EMPTY_LINE)?;
    writeln!(output, "{}", format_device_path(unimplemented!()))?;
    writeln!(output, "{}", EMPTY_LINE)?;
    Ok(output)
}