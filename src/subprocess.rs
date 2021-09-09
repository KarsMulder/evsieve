use std::process::{Command, Stdio, Child};
use std::io;
use std::sync::Mutex;
use signal_hook::iterator::Signals;
use crate::error::{Context, SystemError};

lazy_static! {
    /// Keeps track of all subprocess we've spawned so we can terminate them when evsieve exits.
    static ref MANAGER: Mutex<SubprocessManager> = Mutex::new(SubprocessManager::new());
}

/// Tries to terminate all subprocesses.
pub fn terminate_all() {
    match MANAGER.lock() {
        Ok(mut lock) => lock.terminate_all(),
        Err(_) => eprintln!("Failed to terminate subprocesses: internal lock poisoned."),
    }
}

/// Will spawn a process. The process will be SIGTERM'd when `subprocess::terminate_all` is called
/// (if it is still running by then).
pub fn try_spawn(program: String, args: Vec<String>) -> Result<(), SystemError> {
    // Compute a printable version of the command, so we have something to show the
    // user in case an error happens.
    let printable_cmd: String = vec![program.clone()].into_iter().chain(args.iter().map(
        |arg| if arg.contains(' ') {
            format!("\"{}\"", arg)
        } else {
            arg.clone()
        }
    )).collect::<Vec<String>>().join(" ");

    let child_res: Result<Child, io::Error> =
        Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .spawn();
    let child = match child_res {
        Ok(proc) => proc,
        Err(error) => {
            return Err(SystemError::from(error).with_context(
                format!("While trying to run {}:", printable_cmd)
            ));
        }
    };

    let process = Subprocess {
        child, printable_cmd
    };

    MANAGER.lock().expect("Internal lock poisoned.").add_process(process);
    Ok(())
}

struct SubprocessManager {
    processes: Vec<Subprocess>,
    cleanup_thread_is_running: bool,
}

impl SubprocessManager {
    fn new() -> SubprocessManager {
        SubprocessManager {
            processes: Vec::new(),
            cleanup_thread_is_running: false,
        }
    }

    /// Tries to free the resources of all finished processes.
    fn cleanup(&mut self) {
        self.processes = self.processes.drain(..).filter_map(Subprocess::try_cleanup).collect();
    }

    fn add_process(&mut self, process: Subprocess) {
        self.processes.push(process);

        if ! self.cleanup_thread_is_running {
            if start_cleanup_thread().is_ok() {
                self.cleanup_thread_is_running = true;
            }
        }
    }

    /// Tries to terminate all subprocesses.
    fn terminate_all(&mut self) {
        for process in self.processes.drain(..) {
            process.terminate();
        }
    }
}

struct Subprocess {
    printable_cmd: String,
    child: Child,
}

impl Subprocess {
    /// Tries to wait on self. If the process has exited, then returns None, signalling that the
    /// process has been cleaned up. Otherwise, returns Some(self), signalling that it must be
    /// cleaned up at some later time.
    #[must_use]
    pub fn try_cleanup(mut self) -> Option<Subprocess> {
        match self.child.try_wait() {
            Err(error) => {
                eprintln!("Error while waiting on {}: {}", self.printable_cmd, error);
                None
            },
            Ok(status_opt) => match status_opt {
                // If None, then the subprocess has not exited yet.
                None => Some(self),
                // If Some, then the process has exited.
                Some(status) => if status.success() {
                    None
                } else {
                    match status.code() {
                        Some(code) => eprintln!("Failed to run {}: return code {}.", self.printable_cmd, code),
                        None => eprintln!("Failed to run {}: interrupted by signal.", self.printable_cmd),
                    };
                    None
                }
            }
        }
    }

    pub fn terminate(self) {
        // Make sure the process hasn't already exited before we try to clean it up.
        if let Some(mut process) = self.try_cleanup() {
            // Send a SIGTERM signal.
            unsafe { libc::kill(process.child.id() as i32, libc::SIGTERM) };
            // Wait for it so the operating system cleans up resources.
            std::thread::spawn(move || process.child.wait());
        }
    }
}

fn start_cleanup_thread() -> Result<(), io::Error> {
    let mut signals = Signals::new(&[libc::SIGCHLD])?;
    std::thread::spawn(move || {
        for signal in signals.forever() {
            match signal {
                libc::SIGCHLD => {
                    MANAGER.lock().expect("Internal lock poisoned.").cleanup();
                },
                _ => unreachable!(),
            }
        }
    });
    Ok(())
}