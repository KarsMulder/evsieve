use std::process::{Command, Stdio, Child};
use std::io;
use std::sync::Mutex;

// TODO: processes' exit codes are not checked until a new process is spawned.

lazy_static! {
    /// Keeps track of all subprocess we've spawned so we can terminate them when evsieve exits.
    static ref PROCESSES: Mutex<Vec<Subprocess>> = Mutex::new(Vec::new());
}

/// Tries to free the resources of all finished processes.
pub fn cleanup() {
    let mut processes_lock = PROCESSES.lock().expect("Internal mutex poisoned.");
    *processes_lock = processes_lock.drain(..).filter_map(Subprocess::try_cleanup).collect()
}

/// Tries to terminate all subprocesses.
pub fn terminate_all() {
    let mut processes_lock = PROCESSES.lock().expect("Internal mutex poisoned.");
    for process in processes_lock.drain(..) {
        process.terminate();
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

/// Will spawn a process. Will print an error on failure, but will not return an error code.
pub fn try_spawn(program: String, args: Vec<String>) {
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
            eprintln!("Failed to run {}: {}", printable_cmd, error);
            return;
        }
    };

    let process = Subprocess {
        child, printable_cmd
    };

    cleanup();
    PROCESSES.lock().expect("Internal mutex poisoned.").push(process)
}