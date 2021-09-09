// SPDX-License-Identifier: GPL-2.0-or-later

use std::ffi::CString;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::mem::MaybeUninit;
use std::os::unix::prelude::RawFd;
use std::path::{PathBuf};

use crate::error::{SystemError, Context};

struct PidFile {
    path: PathBuf,
    _file: File,
}

impl PidFile {
    fn new(path: PathBuf) -> Result<PidFile, SystemError> {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)?;

        let pid: i32 = unsafe { libc::getpid() };
        write!(file, "{}", pid)?;
        file.sync_all()?;

        Ok(PidFile {
            path, _file: file
        })
    }
}

impl Drop for PidFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

pub enum Daemon {
    None,
    Traditional(TraditionalDaemon),
}

impl Daemon {
    /// # Safety
    /// This call closes all open file descriptors except the standard I/O. Make sure you do not have
    /// any open file descriptors you intend to use.
    pub unsafe fn traditional(pidfile_path: PathBuf) -> Result<Daemon, SystemError> {
        Ok(Daemon::Traditional(TraditionalDaemon::new(pidfile_path)?))
    }

    pub fn none() -> Daemon {
        Daemon::None
    }

    /// # Safety
    /// This function can only be called from a single-threaded context.
    pub unsafe fn finalize(&mut self) {
        match self {
            Daemon::None => {},
            Daemon::Traditional(daemon) => daemon.finalize(),
        }
    }
}

pub struct TraditionalDaemon {
    _pidfile: PidFile,
    child_pipe: Option<ChildPipe>,
}

impl TraditionalDaemon {
    /// # Safety
    /// This call closes all open file descriptors except the standard I/O. Make sure you do not have
    /// any open file descriptors you intend to write to. Also, all other threads disappear when this
    /// function is called.
    pub unsafe fn new(pidfile_path: PathBuf) -> Result<TraditionalDaemon, SystemError> {
        // Close all open file descriptors except for the standard I/O.
        let rlimit = {
            let mut rlimit_uninit: MaybeUninit<libc::rlimit> = MaybeUninit::uninit();
            let res = libc::getrlimit(libc::RLIMIT_NOFILE, rlimit_uninit.as_mut_ptr());
            match res {
                0 => rlimit_uninit.assume_init(),
                _ => return Err(SystemError::os_with_context("While polling the process' resource limits:")),
            }
        };

        // If the rlim is too big, then there is a choice: either close all of them and take an extraordinary
        // amount of CPU time, or do not do so and risk leaking file descriptors. We decided that the last
        // option is the least harmful one and set an arbitrary limit on the maximum amount of file descriptors
        // that will be closed when daemonizing.
        const MAX_RLIM_FILNO: u64 = 65536;
        for i in 3 ..= rlimit.rlim_cur.min(MAX_RLIM_FILNO) {
            libc::close(i as i32);
        }

        // Prepare pipes for communication with the child.
        // The child should send a byte (i8) equal to the pipe when it is ready. The parent shall then
        // exit with that byte as its status code.
        let mut pipe_fds: [RawFd; 2] = [-1; 2];
        if libc::pipe(&mut pipe_fds as *mut RawFd) < 0 {
            return Err(SystemError::os_with_context("While trying to create internal communication pipes:"));
        }
        let parent_pipe_fd: RawFd = pipe_fds[0];
        let child_pipe_fd: RawFd = pipe_fds[1];

        // Fork the process.
        let pid = libc::fork();
        if pid < 0 {
            // Forking failed.
            return Err(SystemError::os_with_context("While trying to fork the process:"));
        }
        if pid > 0 {
            // Forking was a success. We are the parent.
            libc::close(child_pipe_fd);
            // Wait until the child sends a byte.
            let mut buffer: [i8; 1] = [0; 1];
            loop {
                let result = libc::read(parent_pipe_fd, &mut buffer as *mut _ as *mut libc::c_void, 1);
                if result > 0 {
                    let received_byte = buffer[0];
                    libc::exit(received_byte as i32);
                }
                if result as i32 == -libc::EINTR {
                    continue;
                }
                libc::exit(libc::EXIT_FAILURE);
            }
        }
        // Else: forking was a success. We are the child.
        libc::close(parent_pipe_fd);
        let child_pipe = ChildPipe::from_raw_fd(child_pipe_fd);

        // Create a new session.
        let sid = libc::setsid();
        if sid < 0 {
            return Err(SystemError::os_with_context("While trying to acquire a session ID for the daemon:"));
        }

        // Ignore signals.
        // libc::signal(libc::SIGCHLD, libc::SIG_IGN);
        // libc::signal(libc::SIGHUP, libc::SIG_IGN);

        // Fork a second time.
        let pid = libc::fork();
        if pid < 0 {
            // Forking failed.
            return Err(SystemError::os_with_context("While trying to fork the process a second time:"));
        }
        if pid > 0 {
            // Forking was a success. We are the parent.
            libc::exit(libc::EXIT_SUCCESS);
        }

        libc::umask(0);

        // Change cwd to root directory to avoid blocking mount points from being unmounted.
        let root_dir = CString::new("/").unwrap();
        if libc::chdir(root_dir.as_ptr() as *const i8) < 0 {
            return Err(SystemError::os_with_context("While change the working directory:"));
        }

        let pidfile = PidFile::new(pidfile_path).with_context("While creating the PID file:")?;
        Ok(TraditionalDaemon {
            _pidfile: pidfile,
            child_pipe: Some(child_pipe),
        })
    }

    /// # Safety
    ///
    /// This function can only be called from a single-threaded program. If another thread tries to
    /// write to the standard I/O while this function is running, undefined behaviour ensures.
    pub unsafe fn finalize(&mut self) {
        // Lock Rust's access to the standard I/O so no other safe Rust code dares writing to them
        // while we close and reopen their file descriptors. This does not make the code thread-safe,
        // it just makes is slightly less likely that something really bad happens if some safety
        // assumption is violated.
        let stdin = std::io::stdin();
        let _stdin_lock = stdin.lock();
        let stdout = std::io::stdout();
        let _stdout_lock = stdout.lock();
        let stderr = std::io::stderr();
        let _stderr_lock = stderr.lock();

        // Close the standard I/O and redirect them to /dev/null.
        let devnull = CString::new("/dev/null").unwrap();

        for (fileno, mode) in [(libc::STDIN_FILENO, libc::O_RDONLY),
                               (libc::STDOUT_FILENO, libc::O_WRONLY),
                               (libc::STDERR_FILENO, libc::O_WRONLY)]
        {
            if libc::close(fileno) != 0 {
                libc::exit(libc::EXIT_FAILURE);
            }
            if libc::open(devnull.as_ptr() as *const i8, mode) != fileno {
                libc::exit(libc::EXIT_FAILURE);
            }
        }

        // Inform the parent that the child process is ready.
        if let Some(mut child_pipe) = self.child_pipe.take() {
            let _ = child_pipe.send_byte(0);
        }
    }
}


/// A communication channel from the forked process to the parent process.
struct ChildPipe {
    fd: Option<RawFd>,
}

impl ChildPipe {
    /// # Safety
    /// The file descriptor must be valid.
    unsafe fn from_raw_fd(fd: RawFd) -> ChildPipe {
        ChildPipe { fd: Some(fd) }
    }

    fn send_byte(&mut self, byte: i8) -> Result<(), SystemError> {
        if let Some(fd) = self.fd {
            unsafe {
                let buffer: [i8; 1] = [byte];
                loop {
                    let result = libc::write(fd, &buffer as *const _ as *const libc::c_void, std::mem::size_of::<[i8; 1]>());
                    if result > 0 {
                        libc::close(fd);
                        self.fd = None;
                        return Ok(());
                    }
                    if result < 0 {
                        return Err(SystemError::os_with_context("While communicating with parent process:"))
                    }
                }
            };
        } else {
            Err(SystemError::new("Writing to a closed pipe."))
        }
    }
}

/// In case the ChildPipe is dropped without having sent an "OK" byte yet, the child has probably
/// encountered an error. Send the byte "1" to signal that something is wrong.
impl Drop for ChildPipe {
    fn drop(&mut self) {
        if self.fd.is_some() {
            let _ = self.send_byte(1);
        }
    }
}