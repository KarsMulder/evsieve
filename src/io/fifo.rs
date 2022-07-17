// SPDX-License-Identifier: GPL-2.0-or-later

use std::fmt::Display;
use std::io::{Read};
use std::os::unix::io::{RawFd, AsRawFd};
use std::ffi::CString;
use std::path::{Path, PathBuf};

use crate::error::{SystemError, Context};
use crate::io::fd::{OwnedFd, ReadableFd};

use super::fd::HasFixedFd;

// TODO: Move this structure elsewhere.
struct OwnedPath(PathBuf);

pub trait LineRead : AsRawFd {
    fn read_lines(&mut self) -> Result<Vec<String>, std::io::Error>;
}

impl OwnedPath {
    pub fn new(path: PathBuf) -> OwnedPath {
        OwnedPath(path)
    }
}

impl Display for OwnedPath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0.to_string_lossy())
    }
}

impl Drop for OwnedPath {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

pub struct LineReader<T: Read> {
    /// The device/file/pipe/whatever to read data from.
    source: T,
    /// Bytes that have been read from the source, but not yet emitted to the receiver.
    cached_data: Vec<u8>,
}

impl<T: Read> LineReader<T> {
    pub fn new(source: T) -> Self {
        LineReader {
            source, cached_data: Vec::new()
        }
    }

    /// Performs a single read() call on the underlying source, which may result into reading
    /// zero or more lines in total.
    pub fn read_lines(&mut self) -> Result<Vec<String>, std::io::Error> {
        let mut buf: [u8; libc::PIPE_BUF] = [0; libc::PIPE_BUF];
        let num_bytes_read = match self.source.read(&mut buf) {
            Ok(res) => res,
            Err(error) => match error.kind() {
                std::io::ErrorKind::Interrupted | std::io::ErrorKind::WouldBlock
                    => return Ok(Vec::new()),
                _ => return Err(error),
            }
        };

        self.cached_data.extend_from_slice(&buf[0 .. num_bytes_read]);
        let mut data = self.cached_data.as_slice();
        let mut result = Vec::new();

        // Read lines from the cached data until no lines are left anymore.
        const NEWLINE_DENOMINATOR: u8 = 0xA; // The \n ASCII new line denominator.
        while let Some(newline_index) = linear_search(data, &NEWLINE_DENOMINATOR) {
            let before_newline = &data[0 .. newline_index];
            let after_newline = if data.len() > newline_index + 1 {
                &data[newline_index + 1 .. data.len()]
            } else {
                &[]
            };

            match String::from_utf8(before_newline.to_owned()) {
                Ok(string) => result.push(string),
                Err(_) => {
                    eprintln!("Error: received non-UTF-8 data. Data ignored.");
                }
            }

            data = after_newline;
        }

        self.cached_data = data.to_owned();

        Ok(result)
    }

    pub fn get_buffered_data(&self) -> &[u8] {
        &self.cached_data
    }

    pub fn get_ref(&self) -> &T {
        &self.source
    }
}

/// Represents the reading end of a Fifo that resides on the file system.
/// The file on the filesystem is deleted when the Fifo is dropped.
pub struct Fifo {
    path: OwnedPath,
    reader: LineReader<ReadableFd>,
}

impl Fifo {
    pub fn create(path: &str) -> Result<Fifo, SystemError> {
        let path_cstring = CString::new(path)
            .map_err(|_| SystemError::new("Path may not contain any nul bytes."))?;

        let res = unsafe { libc::mkfifo(path_cstring.as_ptr(), 0o600) };
        if res < 0 {
            return Err(SystemError::os_with_context(format!(
                "While attempting to create a fifo at {}:", path
            )));
        }

        let owned_path = OwnedPath::new(path.into());
        let fd = unsafe {
            OwnedFd::from_syscall(
                libc::open(path_cstring.as_ptr(), libc::O_RDONLY | libc::O_NONBLOCK)
            ).with_context_of(|| format!(
                "While trying to open the fifo at {}:", path
            ))?
            .readable()
        };
        let reader = LineReader::new(fd);

        Ok(Fifo { path: owned_path, reader })
    }

    pub fn path(&self) -> &Path {
        &self.path.0
    }
}

impl LineRead for Fifo {
    /// Returns all lines that are ready for this Fifo.
    /// The lines shall not end at a \n character.
    /// This function returns all lines that are available and shall not return any more lines
    /// until the epoll says that it ise ready again.
    fn read_lines(&mut self) -> Result<Vec<String>, std::io::Error> {
        let lines = self.reader.read_lines()?;

        if ! self.reader.get_buffered_data().is_empty() {
            // TODO: this blatantly assumes that the Fifo is used as command fifo.
            // TODO: Also, this somehow does not work. Figure out why.
            let partial_command = String::from_utf8_lossy(self.reader.get_buffered_data());
            eprintln!("Error: received a command \"{}\" that was not terminated by a newline character. All commands must be terminated by newline characters.", partial_command);
        }

        Ok(lines)
    }
}

impl AsRawFd for Fifo {
    fn as_raw_fd(&self) -> RawFd {
        self.reader.get_ref().as_raw_fd()
    }
}

unsafe impl HasFixedFd for Fifo {}

/// Returns the index of the first instance of `search_elem` in the provided slice, or `None`
/// if it is not found in said slice.
fn linear_search<T : Eq>(container: &[T], search_elem: &T) -> Option<usize> {
    for (index, elem) in container.iter().enumerate() {
        if elem == search_elem {
            return Some(index)
        }
    }

    None
}
