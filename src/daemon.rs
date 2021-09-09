// SPDX-License-Identifier: GPL-2.0-or-later

use std::ffi::CString;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::mem::MaybeUninit;
use std::path::{PathBuf};

use crate::error::{SystemError, Context};

struct PidFile {
    path: PathBuf,
    file: File,
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
            path, file
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
    /// any open file descriptors you intend to write to.
    pub unsafe fn traditional(pidfile_path: PathBuf) -> Result<Daemon, SystemError> {
        Ok(Daemon::Traditional(TraditionalDaemon::new(pidfile_path)?))
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
    pidfile: PidFile,
}

impl TraditionalDaemon {
    /// # Safety
    /// This call closes all open file descriptors except the standard I/O. Make sure you do not have
    /// any open file descriptors you intend to write to.
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

        // Fork the process.
        let pid = libc::fork();
        if pid < 0 {
            // Forking failed.
            return Err(SystemError::os_with_context("While trying to fork the process:"));
        }
        if pid > 0 {
            // Forking was a success. We are the parent.
            libc::exit(libc::EXIT_SUCCESS);
        }
        // Else: forking was a success. We are the child.

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
        Ok(TraditionalDaemon { pidfile })
    }

    /// # Safety
    ///
    /// This function can only be called from a single-threaded program. If another thread tries to
    /// write to the standard I/O while this function is running, undefined behaviour ensures.
    pub unsafe fn finalize(&mut self) {
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
    }
}
