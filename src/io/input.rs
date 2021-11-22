// SPDX-License-Identifier: GPL-2.0-or-later

use std::ffi::{CStr, CString};
use std::fs::{File, OpenOptions};
use std::os::unix::fs::OpenOptionsExt;
use std::os::unix::io::{AsRawFd, RawFd};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use crate::activity::ActivityLink;
use crate::bindings::libevdev;
use crate::event::{Event, EventType, EventValue, EventCode, Namespace};
use crate::domain::Domain;
use crate::capability::{Capability, Capabilities, AbsInfo, RepeatInfo};
use crate::ecodes;
use crate::predevice::{GrabMode, PersistMode, PreInputDevice};
use crate::error::{SystemError, Context};
use crate::persist::blueprint::Blueprint;

use super::fd::HasFixedFd;

pub fn open_and_query_capabilities(pre_input_devices: Vec<PreInputDevice>)
    -> Result<(Vec<InputDevice>, Vec<Capability>), SystemError>
{
    let mut input_devices = pre_input_devices.into_iter().map(
        |device| {
            let device_path = device.path.clone();
            InputDevice::open(device)
                .map_err(SystemError::from)
                .with_context(format!("While opening the device \"{}\":", device_path.display()))
    }).collect::<Result<Vec<InputDevice>, SystemError>>()?;

    // Return an error if a device with grab=force cannot be grabbed.
    for device in &mut input_devices {
        device.grab_if_desired()?;
    }

    // Precompute the capabilities of the input devices.
    let mut capabilities_vec: Vec<Capability> = Vec::new();
    for device in &input_devices {
        let mut device_capabilities_vec = device.capabilities.to_vec_from_domain_and_namespace(device.domain, Namespace::Input);
        capabilities_vec.append(&mut device_capabilities_vec);
    }

    Ok((input_devices, capabilities_vec))
}

// Represents a name as reported by libevdev_get_name().
pub type InputDeviceName = CString;

pub struct InputDevice {
    /// The file owns the file descriptor to the input device. Beware: InputDevice implements HasFixedFd.
    file: File,
    path: PathBuf,
    evdev: *mut libevdev::libevdev,

    capabilities: Capabilities,

    /// The name as reported by libevdev_get_name().
    name: InputDeviceName,

    /// Whether and how the user has requested this device to be grabbed.
    grab_mode: GrabMode,
    /// Whether the device is actually grabbed.
    grabbed: bool,

    /// The domain, though not part of libevdev, is a handy tag we use
    /// to track which device emitted the events.
    domain: Domain,

    /// Maps (type, code) pairs to the last known value of said pair.
    state: HashMap<EventCode, EventValue>,

    /// What should happen if this device disconnects.
    persist_mode: PersistMode,

    /// Prevents automatic exit of evsieve as long as one InputDevice exists.
    _activity_link: ActivityLink,
}

impl InputDevice {
    /// Opens an input device from a given path.
    ///
    /// Does not grab the device even if grab=force is specified. You must do that manually later
    /// by calling grab_if_desired().
    pub fn open(pre_device: PreInputDevice) -> Result<InputDevice, SystemError> {
        let path = pre_device.path;
        let domain = pre_device.domain;

        // Open the file itself.
        let file = OpenOptions::new()
            .read(true)
            // O_CLOEXEC is already set by default in the std source code, but I'm providing it
            // anyway to clearly signify we _need_ that flag.
            .custom_flags(libc::O_NONBLOCK | libc::O_CLOEXEC)
            .open(&path)?;

        // Turn the file into an evdev instance.
        let mut evdev: *mut libevdev::libevdev = std::ptr::null_mut();
        let res = unsafe {
            libevdev::libevdev_new_from_fd(file.as_raw_fd(), &mut evdev)
        };
        if res < 0 {
            return Err(SystemError::new(
                format!("Failed to open a libevdev instance: {}.", path.to_string_lossy())
            ));
        }

        let capabilities = unsafe { get_capabilities(evdev) };
        let state = unsafe { get_device_state(evdev, &capabilities) };

        // According to the documentation, libevdev_get_name() never returns a null pointer
        // but may return an empty string. We are not sure whether the return value is guaranteed
        // to be UTF-8 decodable, so it may be possible that device_name ends up as None.
        let name: InputDeviceName = unsafe {
            CStr::from_ptr(libevdev::libevdev_get_name(evdev))
        }.to_owned();

        Ok(InputDevice {
            file, path, evdev, domain, capabilities, state, name,
            grab_mode: pre_device.grab_mode, grabbed: false,
            persist_mode: pre_device.persist_mode,
            _activity_link: ActivityLink::new(),
        })
    }

    pub fn domain(&self) -> Domain {
        self.domain
    }

    fn read_raw(&mut self) -> Result<Vec<(EventCode, EventValue)>, SystemError> {
        let mut event: libevdev::input_event = unsafe { std::mem::zeroed() };
        let mut should_sync = false;
        let mut events: Vec<(EventCode, EventValue)> = Vec::new();

        loop {
            let flags = match should_sync {
                true => libevdev::libevdev_read_flag_LIBEVDEV_READ_FLAG_SYNC,
                false => libevdev::libevdev_read_flag_LIBEVDEV_READ_FLAG_NORMAL,
            };
            let res = unsafe {
                libevdev::libevdev_next_event(self.evdev, flags, &mut event)
            };

            const SUCCESS: i32 = libevdev::libevdev_read_status_LIBEVDEV_READ_STATUS_SUCCESS as i32;
            const SYNC: i32 = libevdev::libevdev_read_status_LIBEVDEV_READ_STATUS_SYNC as i32;
            const MINUS_EAGAIN: i32 = -libc::EAGAIN;
            const MINUS_EINTR: i32 = -libc::EINTR;

            let event_type = unsafe { EventType::new(event.type_) };
            let event_code = unsafe { EventCode::new(event_type, event.code) };

            match res {
                SUCCESS => events.push((event_code, event.value)),
                SYNC => {
                    events.push((event_code, event.value));
                    should_sync = true;
                },
                MINUS_EAGAIN => break,
                MINUS_EINTR => break,
                _ => return Err(SystemError::new(
                    "An unknown error occured while reading from an event device."
                )),
            }
        }

        Ok(events)
    }

    /// Given an event code and value, creates an `Event` that has all entries filled
    /// out as if it was a real event that was received by this input device. Updates
    /// the state of `self` as if this event was really received. The resulting event
    /// shall be directly returned by this function; it will not be queried to be
    /// returned by `poll()`.
    ///
    /// This function is public and is callable from both this class' member functions
    /// to process real events, as well as from other parts in the code to simulate
    /// having received events.
    pub fn synthesize_event(&mut self, code: EventCode, value: EventValue) -> Event {
        let previous_value_mut: &mut EventValue = self.state.entry(code).or_insert(0);
        let previous_value: EventValue = *previous_value_mut;
        *previous_value_mut = value;
        Event::new(
            code, value, previous_value, self.domain, Namespace::Input,
        )
    }

    /// Reads the raw events from the device and attached additional information such as the
    /// domain of this device and whatever value this event had the last time it was seen.
    pub fn poll(&mut self) -> Result<Vec<Event>, SystemError> {
        let events: Vec<Event> = self.read_raw()?
            .into_iter()
            .map(|(code, value)| self.synthesize_event(code, value))
            .collect();

        self.grab_if_desired()?;
        Ok(events)
    }

    /// Tries to grab the device if grab_mode says we should.
    ///
    /// Returns Ok if either grabbing was successful or there is no need to grab this device.
    /// Returns Err(SystemError) if we tried to grab the device, but failed because the OS didn't
    /// let us grab the device.
    pub fn grab_if_desired(&mut self) -> Result<(), SystemError> {
        if self.grabbed {
            return Ok(());
        }
        match self.grab_mode {
            GrabMode::None => Ok(()),
            GrabMode::Force => self.grab(),
            GrabMode::Auto => {
                // Grab if no key is currently pressed.
                if self.get_pressed_keys().count() > 0 {
                    return Ok(());
                }
                self.grab()
            }
        }
    }

    /// Returns an iterator of all EV_KEY codes that are currently pressed.
    pub fn get_pressed_keys(&self) -> impl Iterator<Item=EventCode> + '_ {
        self.state.iter()
            .filter(|(code, value)| code.ev_type().is_key() && **value > 0)
            .map(|(&code, &_value)| code)
    }

    fn grab(&mut self) -> Result<(), SystemError> {
        let res = unsafe {
            libevdev::libevdev_grab(self.evdev, libevdev::libevdev_grab_mode_LIBEVDEV_GRAB)
        };
        if res < 0 {
            Err(SystemError::new(
                format!("Failed to grab input device: {}", self.path.to_string_lossy()
            )))
        } else {
            self.grabbed = true;
            Ok(())
        }
    }

    fn ungrab(&mut self) -> Result<(), SystemError> {
        let res = unsafe {
            libevdev::libevdev_grab(self.evdev, libevdev::libevdev_grab_mode_LIBEVDEV_GRAB)
        };
        if res < 0 {
            Err(SystemError::new(
                format!("Failed to ungrab event device: {}", self.path.to_string_lossy()
            )))
        } else {
            self.grabbed = false;
            Ok(())
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn capabilities(&self) -> &Capabilities {
        &self.capabilities
    }

    pub fn name(&self) -> &InputDeviceName {
        &self.name
    }

    pub fn persist_mode(&self) -> PersistMode {
        self.persist_mode
    }

    // Closes the device and returns a blueprint from which it can be reopened.
    pub fn to_blueprint(self) -> Blueprint {
        Blueprint {
            capabilities: self.capabilities.clone(),
            name: self.name.clone(),
            pre_device: PreInputDevice {
                path: self.path.clone(),
                grab_mode: self.grab_mode,
                domain: self.domain,
                persist_mode: self.persist_mode,
            },
            activity_link: ActivityLink::new(),
        }
    }
}

/// This implement is necessary becaus *mut libevdev::libevdev is not Send.
unsafe impl Send for InputDevice {}

/// # Safety
/// Exhibits undefined behaviour if evdev is not a valid pointer.
unsafe fn get_capabilities(evdev: *mut libevdev::libevdev) -> Capabilities {
    let event_types = ecodes::EVENT_TYPES.values().cloned();
    let event_codes = ecodes::EVENT_CODES.values().cloned();
    
    let supported_event_types: HashSet<EventType> = event_types.filter(|&ev_type| {
        libevdev::libevdev_has_event_type(evdev, ev_type.into()) == 1
    }).collect();

    let supported_event_codes: HashSet<EventCode> = event_codes
        .filter(|&code| supported_event_types.contains(&code.ev_type()))
        .filter(|&code| {
            libevdev::libevdev_has_event_code(evdev, code.ev_type().into(), code.code() as u32) == 1
        }).collect();
    
    // Query the abs_info from this device.
    let mut abs_info: HashMap<EventCode, AbsInfo> = HashMap::new();
    for &code in &supported_event_codes {
        if code.ev_type().is_abs() {
            let evdev_abs_info: *const libevdev::input_absinfo = libevdev::libevdev_get_abs_info(evdev, code.code() as u32);
            abs_info.insert(code, (*evdev_abs_info).into());
        }
    }

    // Query rep_info from this device.
    let rep_info = {
        let mut delay: libc::c_int = 0;
        let mut period: libc::c_int = 0;
        let res = libevdev::libevdev_get_repeat(evdev, &mut delay as *mut libc::c_int, &mut period as *mut libc::c_int);
        match res {
            0 => Some(RepeatInfo { delay, period }),
            _ => None,
        }
    };

    Capabilities {
        codes: supported_event_codes,
        abs_info,
        rep_info,
    }
}

/// # Safety
/// Exhibits undefined behaviour if evdev is not a valid pointer or the capabilities are invalid.
unsafe fn get_device_state(evdev: *mut libevdev::libevdev, capabilities: &Capabilities) -> HashMap<EventCode, EventValue> {
    let mut device_state: HashMap<EventCode, EventValue> = HashMap::new();
    for &code in &capabilities.codes {
        // ISSUE: ABS_MT support
        if ! ecodes::is_abs_mt(code) {
            let value: i32 = libevdev::libevdev_get_event_value(evdev, code.ev_type().into(), code.code() as u32);
            device_state.insert(code, value);
        } else {
            // The return value of libevdev_get_event_value() for ABS_MT_* is undefined. Until we
            // get proper ABS_MT support, we'll use an arbitrary placeholder value.
            let value = match capabilities.abs_info.get(&code) {
                Some(abs_info) => 
                    EventValue::checked_add(abs_info.min_value, abs_info.max_value)
                        .map(|x| x / 2).unwrap_or(0),
                None => 0,
            };
            device_state.insert(code, value);
        }
        
    }
    device_state
}

impl AsRawFd for InputDevice {
    fn as_raw_fd(&self) -> RawFd {
        self.file.as_raw_fd()
    }
}
unsafe impl HasFixedFd for InputDevice {}

impl Drop for InputDevice {
    fn drop(&mut self) {
        if self.grabbed {
            // Even if the ungrab fails, there's nothing we can do, so we ignore a possible error.
            let _ = self.ungrab();
        }

        unsafe {
            // This does not close the file descriptor itself. That part happens when
            // self.file gets dropped.
            libevdev::libevdev_free(self.evdev);
        }
    }
}
