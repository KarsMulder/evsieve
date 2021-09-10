use std::os::raw::{c_int, c_char};

#[cfg(feature = "systemd")]
extern "C" {
    fn sd_notify(unset_environment: c_int, state: *const c_char) -> c_int;
}

#[cfg(feature = "systemd")]
fn notify(state: &str) {
    let state_cstring = std::ffi::CString::new(state).unwrap();
    let _result = unsafe { sd_notify(0, state_cstring.as_ptr()) };
}

#[cfg(not(feature = "systemd"))]
fn notify(state: &str) {
    // Do nothing.
}

pub fn notify_ready() {
    notify("READY=1")
}

pub fn is_available() -> bool {
    if cfg!(feature = "systemd") {
        std::env::var("NOTIFY_SOCKET").is_ok()
    } else {
        false
    }
}
