use std::fs::OpenOptions;
use std::io::{BufRead, BufReader};

use crate::error::{Context, SystemError};

use super::hid_usage::{HidPage, HidUsage};

enum LoadTablesResult {
    Ok(Vec<HidPage>),
    /// Some IO error occurred while trying to read the tables.
    IoError { path: &'static str, err: std::io::Error },
    /// The HID usage tables to not seem to be installed on the user's system, or are not available at
    /// the expected paths to them.
    NotFound,
    /// We loaded the file, but couldn't extract any USB usages from it. It probably has an unexpected format.
    Empty { path: &'static str },
}

// Loads the USB HID usage tables, and if it fails, prints a suitable error message to stderr.
pub fn load_tables_and_print_error() -> Option<Vec<HidPage>> {
    match load_tables() {
        LoadTablesResult::Ok(tables) => Some(tables),
        LoadTablesResult::IoError { path, err } => {
            SystemError::from(err).with_context(format!("While trying to load the USB HID usage descriptions from {}:", path))
                .print_err();
            None
        },
        LoadTablesResult::NotFound => {
            eprintln!("Evsieve cannot find any tables containing USB HID usage descriptions on your system. On most Linux distributions, those tables are contained in a package called \"hwdata\". If you have already installed that package and still encounter this message, please file a bug report at and mention which distribution you use.");
            None
        },
        LoadTablesResult::Empty { path } => {
            eprintln!("Evsieve tried to read the USB HID usage descriptions from {}, but didn't find any. Either the HID descriptions are not contained in that file, or the file has an unexpected file format. Please file a bug report at https://github.com/KarsMulder/evsieve/issues and mention which distribution you use.", path);
            None
        },        
    }
}

fn load_tables() -> LoadTablesResult {
    let possible_usb_table_locations = [
        "/usr/share/hwdata/usb.ids",
        "/usr/share/misc/usb.ids"
    ];

    for path in possible_usb_table_locations {
        match OpenOptions::new().read(true).write(false).create(false).open(path) {
            Ok(file) => {
                let reader = BufReader::new(file);

                return match parse_tables(reader) {
                    Ok(tables) => {
                        // Do a sanity check on the parsed tables. If we didn't encounter any usage info,
                        // then something probably went wrong.
                        let num_usages_found: usize = tables.iter().map(|page| page.usages.len()).sum();
                        if num_usages_found == 0 {
                            return LoadTablesResult::Empty { path };
                        }
                    
                        LoadTablesResult::Ok(tables)
                    },
                    Err(err) => LoadTablesResult::IoError { path, err },
                }
            },
            Err(err) => match err.kind() {
                // If not found: just try the next possible location.
                std::io::ErrorKind::NotFound => (),
                // These errors are more serious.
                _ => return LoadTablesResult::IoError { path, err },
            }
        }
    }

    return LoadTablesResult::NotFound;
}

/// Reads data from a source and directly turns it into data. The only error case is when we fail to read data
/// from the BufRead. The file format is not validated.
fn parse_tables(mut source: impl BufRead) -> Result<Vec<HidPage>, std::io::Error> {
    // While we read lines from the source, we will first encounter a header declaring the start of a new
    // page, and then the lines that follow will explain which usages belong to that page. Between the point
    // where the header arrives and the point where the last usage arrives, the page is in "partial" state.
    let mut partial_hid_page_opt: Option<HidPage> = None;

    // Whenever a page is in partial state, and a line (or EOF) arrives that does not add another usage to
    // the partial page, the page is finalized and added to the list of complete hid pages.
    let mut complete_hid_pages: Vec<HidPage> = Vec::new();

    let finalize_hid_page = |partial_hid_page_opt: &mut Option<HidPage>, complete_hid_pages: &mut Vec<HidPage>| {
        if let Some(mut page) = partial_hid_page_opt.take() {
            page.usages.sort_by_key(|usage| usage.id);
            complete_hid_pages.push(page);
        }
    };

    let mut buf: Vec<u8> = Vec::new();
    loop {
        buf.clear();
        // We can't just use read_line() because it appearst that this file may contain
        // invalid UTF-8 data.
        match source.read_until(b'\n', &mut buf) {
            Ok(0) => break,
            Ok(_) => {},
            Err(err) => return Err(err),
        }

        buf.pop(); // Remove the trailing newline character.
        let line = String::from_utf8_lossy(&buf);

        // Check for a page header such as "HUT 0b  Telephony"
        if let ("HUT", Some(remainder)) = take_word(&line) {
            if let (page_id_str, Some(page_name)) = take_word(&remainder) {
                if let Ok(page_id) = u16::from_str_radix(page_id_str, 16) {
                    finalize_hid_page(&mut partial_hid_page_opt, &mut complete_hid_pages);
                    partial_hid_page_opt = Some(HidPage {
                        id: page_id,
                        name: page_name.to_owned(),
                        usages: Vec::new(),
                    });
                }
            }
            continue;
        }

        // After a header, usages follow. One tab preceeds each line of usages. More
        // whitespace may follow after the tab, so we cant just use `strip_prefix`.
        // Example: "\t 000  Unassigned"
        if let Some(ref mut partial_hid_page) = &mut partial_hid_page_opt {
            if line.starts_with('\t') {
                let remainder = line.trim_start_matches(|c: char| c.is_ascii_whitespace());
                if let (usage_id_str, Some(usage_name)) = take_word(remainder) {
                    if let Ok(usage_id) = u16::from_str_radix(usage_id_str, 16) {
                        partial_hid_page.usages.push(HidUsage {
                            id: usage_id,
                            name: usage_name.to_owned(),
                        });
                    }
                }
            } else {
                finalize_hid_page(&mut partial_hid_page_opt, &mut complete_hid_pages);
            }
        }
    }

    finalize_hid_page(&mut partial_hid_page_opt, &mut complete_hid_pages);
    complete_hid_pages.sort_by_key(|page| page.id);

    Ok(complete_hid_pages)
}

/// Returns the first word in the string and the rest of the string, skipping over the whitespace
/// between the word and the rest of the sentence. The second option is None if no whitespace
/// follows the first word. If whitespace follows but no word comes after the whitespace, the
/// second argument will be Some(&"").
fn take_word(input: &str) -> (&str, Option<&str>) {
    let first_whitespace_index = match input.find(|c: char| c.is_ascii_whitespace()) {
        Some(idx) => idx,
        None => return (input, None),
    };

    let (start, remainder) = input.split_at(first_whitespace_index);
    let trimmed_remainder = remainder.trim_start_matches(|c: char| c.is_ascii_whitespace());
    (start, Some(trimmed_remainder))
}
