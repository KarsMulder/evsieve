// SPDX-License-Identifier: GPL-2.0-or-later

use std::fmt::Display;
use std::io::Read;
use std::mem::MaybeUninit;
use std::os::unix::io::{RawFd, AsRawFd};
use std::ffi::CString;
use std::path::PathBuf;

use crate::error::{SystemError, Context};
use crate::io::fd::{OwnedFd, ReadableFd};

use super::fd::HasFixedFd;

// TODO: LOW-PRIORITY: Move this structure elsewhere.
struct OwnedPath(PathBuf);

/// Represents a path that we may or may not own. If we own it, the file at the path will be removed
/// when this structure goes out of scope.
enum MaybeOwnedPath {
    Owned(OwnedPath),
    NotOwned(PathBuf),
}

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
    _path: MaybeOwnedPath,
    reader: LineReader<ReadableFd>,
}

impl Fifo {
    pub fn open_or_create(path: &str) -> Result<Fifo, SystemError> {
        // Open the FIFO if it already exists on the filesystem, or create a new one if it doesn't.
        let fd = match try_open_fifo(path) {
            TryOpenFifoResult::Ok(fd) => fd,
            TryOpenFifoResult::Err(error) => {
                return Err(error.with_context_of(|| format!("While trying to open the FIFO at {}:", path)));
            },
            TryOpenFifoResult::NotFound => {
                // There is no FIFO at the specified path. Create a new one.
                return Fifo::create(path);
            },
            TryOpenFifoResult::NonFifoFileEncountered => {
                crate::utils::warn_once(format!("Warning: a file already exists at {}, but that file is not a FIFO. That file will be deleted and replaced by a FIFO.", path));
                std::fs::remove_file(path).map_err(SystemError::from).with_context_of(
                    || format!("While trying to remove the file at {}:", path)
                )?;
                return Fifo::create(path);
            },
        };

        let reader = LineReader::new(unsafe { fd.readable() });
        Ok(Fifo { _path: MaybeOwnedPath::NotOwned(path.into()), reader })
    }

    /// Creates a new FIFO. Does not handle the case where a FIFO already exists at the provided path.
    /// Used as an inner fallback function for `open_or_create()`. Other code should call that function instead.
    fn create(path: &str) -> Result<Fifo, SystemError> {
        let path_cstring = CString::new(path)
            .map_err(|_| std::io::Error::new(std::io::ErrorKind::InvalidInput, "Path may not contain any NUL bytes."))?;

        let res = unsafe { libc::mkfifo(path_cstring.as_ptr(), 0o600) };
        if res < 0 {
            return Err(SystemError::os_with_context(format!(
                "While attempting to create a fifo at {}:", path
            )));
        }

        let fd = match try_open_fifo(path) {
            TryOpenFifoResult::Ok(fd) => fd,
            TryOpenFifoResult::Err(err) => return Err(err.with_context_of(|| format!("While trying to open the newly created FIFO at {}:", path))),
            TryOpenFifoResult::NotFound => return Err(SystemError::new(format!("We created a new FIFO at {}, but received a \"file not found\" error when we tried to open it.", path))),
            TryOpenFifoResult::NonFifoFileEncountered => return Err(SystemError::new(format!("We created a new FIFO at {}, but when we tried to open it, the OS told us that the file at that location was not a FIFO.", path))),
            
        };

        let reader = LineReader::new(unsafe { fd.readable() });
        Ok(Fifo { _path: MaybeOwnedPath::Owned(OwnedPath::new(path.into())), reader })
    }
}

/// Enumerates the possible return values of `try_open_fifo()`. Contains both generic errors which should be
/// bubbled up, as well as specific errors that need to be handled.
enum TryOpenFifoResult {
    Ok(OwnedFd),
    Err(SystemError),

    /// No file exists at the provided path.
    NotFound,
    /// This error shall be returned in case we successfully opened a file, but that file was not a FIFO.
    NonFifoFileEncountered,
}
fn try_open_fifo(path: &str) -> TryOpenFifoResult {
    let path_cstring = match CString::new(path) {
        Ok(value) => value,
        Err(_) => return TryOpenFifoResult::Err(std::io::Error::new(std::io::ErrorKind::InvalidInput, "Path may not contain any NUL bytes.").into()),
    };

    let fd = unsafe {
        // Workaround suggested by:
        //     https://stackoverflow.com/questions/22021253/poll-on-named-pipe-returns-with-pollhup-constantly-and-immediately
        //
        // You might think that we should open this epoll with O_RDONLY because we only ever read
        // from it. However, the Linux kernel devs, in their infinite wisdom, decided that whenever
        // an FIFO gets closed by its last writer, it generates an EPOLLHUP event which is not
        // cleared after being read from the epoll, and which does not seem to be clearable by any
        // less-than-farfetched means. (Or at least, I haven't found a good way to clear it yet.)
        // Consequently, a level-triggered epoll will immediately return from any subsequent
        // `epoll_wait()` calls, resulting in a busy loop consuming 100% CPU.
        //
        // Other than switching to an edge-triggered epoll (which is another whole can of worms)
        // the best workaround I found seems to be to open the FIFO for writing ourselves, which
        // ensures that the last writer (us) never closes the FIFO and thereby preventing that
        // EPOLLHUP event from happening.
        //
        // Hence the O_RDWR mode.
        let res = libc::open(path_cstring.as_ptr(), libc::O_RDWR | libc::O_NONBLOCK);
        if res < 0 {
            let error = std::io::Error::last_os_error();
            match error.kind() {
                std::io::ErrorKind::NotFound => return TryOpenFifoResult::NotFound,
                _ => return TryOpenFifoResult::Err(error.into()),
            }
        } else {
            OwnedFd::new(res)
        }
    };

    // Check that the thing we just opened is really a FIFO. This is handy in case we reuse a FIFO from the
    // filesystem, but even if we just created one, we could be subject to race conditions.
    let mut stat: MaybeUninit<libc::stat> = MaybeUninit::uninit();
    let res = unsafe { libc::fstat(fd.as_raw_fd(), stat.as_mut_ptr()) };
    if res < 0 {
        return TryOpenFifoResult::Err(SystemError::os_with_context("While attempting to retrieve metadata of the file:"));
    }

    let stat = unsafe { stat.assume_init() };
    if stat.st_mode & libc::S_IFMT != libc::S_IFIFO {
        return TryOpenFifoResult::NonFifoFileEncountered;
    }

    // TODO (feature control-fifo): The presence of a control FIFO should keep evsieve from exiting by inactivity.
    // Check if the FIFO is owned by root or the user evsieve is running as.
    let my_uid = unsafe { libc::geteuid() };
    let is_running_as_root = my_uid == 0;
    if stat.st_uid != 0 && stat.st_uid != my_uid {
        print_security_warning();
        if is_running_as_root {
            return TryOpenFifoResult::Err(SystemError::new("This FIFO is not owned by root."));
        } else {
            return TryOpenFifoResult::Err(SystemError::new("This FIFO is owned by neither root nor the user that evsieve is running as."));
        }
    }

    // Check if the permissions on the FIFO are acceptable.
    if stat.st_mode & (libc::S_IXUSR | libc::S_IXGRP | libc::S_IXOTH) != 0 {
        print_security_warning();
        return TryOpenFifoResult::Err(SystemError::new("This FIFO is marked as executable in its permission bits."));
    }
    if stat.st_mode & (libc::S_IROTH | libc::S_IWOTH) != 0 {
        print_security_warning();
        return TryOpenFifoResult::Err(SystemError::new("This FIFO is read- or writable by others. This is a security hole."));
    }

    TryOpenFifoResult::Ok(fd)
}

fn print_security_warning() {
    crate::utils::warn_once("INFO: although the current capabilities of the control FIFO are quite limited, they may be expanded into the future. Any user who obtains write access to the control FIFO should be assumed to be capable of assuming complete control over the evsieve process, and therefore be capable of arbitrary code execution under the account that evsieve is running as. Since evsieve is usually running as root, that means that anyone who obtains write access to the control FIFO has effectively root access. Under most circumstances, the control FIFO should only be writable by root. To avoid accidental foot-shooting, evsieve makes some basic sanity checks on the permissions of the control FIFO. These checks are:\n\n    1. The FIFO must be owned by either root, or the user that evsieve is running as;\n    2. The permissions on the FIFO must not exceed 660, i.e. not executable by anyone, and not read- or writable by others.\n\nYou are recommended to assign more restrictive permissions to the FIFO to avoid future security holes.\n");
}

impl LineRead for Fifo {
    /// Returns all lines that are ready for this Fifo.
    /// The lines shall not end at a \n character.
    /// This function returns all lines that are available and shall not return any more lines
    /// until the epoll says that it ise ready again.
    fn read_lines(&mut self) -> Result<Vec<String>, std::io::Error> {
        let lines = self.reader.read_lines()?;

        if ! self.reader.get_buffered_data().is_empty() {
            // TODO: FEATURE(control-fifo) this blatantly assumes that the Fifo is used as command fifo.
            // TODO: FEATURE(control-fifo) Also, this somehow does not work. Figure out why.
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
