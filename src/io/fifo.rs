use std::fmt::Display;
use std::fs::File;
use std::io::{BufReader, BufRead};
use std::os::unix::prelude::{AsRawFd, FromRawFd};
use std::ffi::{CString, CStr};
use std::path::PathBuf;

use crate::error::{SystemError, Context};
use crate::io::fd::{OwnedFd, ReadableFd};

use super::fd::HasFixedFd;

// TODO: Move this structure elsewhere.
struct OwnedPath(PathBuf);

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

/// Represents the reading end of a Fifo that resides on the file system.
/// The file on the filesystem is deleted when the Fifo is dropped.
pub struct Fifo {
    path: OwnedPath,
    reader: BufReader<ReadableFd>,
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
        let reader = BufReader::new(fd);

        Ok(Fifo { path: owned_path, reader })
    }

    pub fn read_lines(&mut self) -> Result<Vec<String>, SystemError> {
        let mut lines: Vec<String> = Vec::new();
        loop {
            let mut line: String = String::new();
            let bytes_read = self.reader.read_line(&mut line)
                .map_err(SystemError::from)
                .with_context_of(|| format!("While reading from the fifo {}:", self.path))?;
            if bytes_read == 0 {
                break;
            }

            if line.ends_with('\n') {
                lines.push(line);
            } else {
                // TODO: this blatantly assumes that the Fifo is used as command fifo.
                eprintln!("Error: received a command \"{}\" that was not terminated by a newline character. All commands must be terminated by newline characters.", line);
            }
        }

        Ok(lines)
    }
}

impl AsRawFd for Fifo {
    fn as_raw_fd(&self) -> std::os::unix::prelude::RawFd {
        self.reader.get_ref().as_raw_fd()
    }
}

unsafe impl HasFixedFd for Fifo {}
