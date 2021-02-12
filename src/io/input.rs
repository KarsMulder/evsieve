// SPDX-License-Identifier: GPL-2.0-or-later

use std::fs::{File, OpenOptions};
use std::os::unix::fs::OpenOptionsExt;
use std::os::unix::io::{AsRawFd, RawFd};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use crate::bindings::libevdev;
use crate::io::epoll::Epoll;
use crate::io::persist::{InputDeviceBlueprint};
use crate::event::{Event, EventType, EventValue, EventCode, Namespace};
use crate::domain::Domain;
use crate::capability::{Capability, Capabilities, AbsInfo, RepeatInfo};
use crate::ecodes;
use crate::predevice::{GrabMode, PersistMode, PreInputDevice};
use crate::error::{SystemError, Context};
use crate::io::epoll::Pollable;
use crate::io::persist::BlueprintOpener;

pub fn open_and_query_capabilities(pre_input_devices: Vec<PreInputDevice>)
    -> Result<(Epoll, Vec<Capability>), SystemError>
{
    let input_devices = pre_input_devices.into_iter().map(
        |device| {
            let device_path = device.path.clone();
            InputDevice::open(device)
                .map_err(SystemError::from)
                .with_context(format!("While opening the device \"{}\":", device_path.display()))
    }).collect::<Result<Vec<InputDevice>, SystemError>>()?;

    // Precompute the capabilities of the input devices.
    let mut capabilities_vec: Vec<Capability> = Vec::new();
    for device in &input_devices {
        let mut device_capabilities_vec = device.capabilities.to_vec_from_domain_and_namespace(device.domain, Namespace::Input);
        capabilities_vec.append(&mut device_capabilities_vec);
    }

    let mut epoll = Epoll::new()?;
    for device in input_devices {
        unsafe { epoll.add_file(Box::new(device))? };
    }

    Ok((epoll, capabilities_vec))
}

pub struct InputDevice {
    file: File,
    path: PathBuf,
    evdev: *mut libevdev::libevdev,

    capabilities: Capabilities,

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
}

impl InputDevice {
    pub fn open(pre_device: PreInputDevice) -> Result<InputDevice, SystemError> {
        let path = pre_device.path;
        let domain = pre_device.domain;

        // Open the file itself.
        let file = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NONBLOCK)
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

        let mut device = InputDevice {
            file, path, evdev, domain, capabilities, state,
            grab_mode: pre_device.grab_mode, grabbed: false,
            persist_mode: pre_device.persist_mode,
        };
        device.grab_if_desired()?;

        Ok(device)
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
            const EAGAIN: i32 = -libc::EAGAIN;

            let event_type = unsafe { EventType::new(event.type_) };
            let event_code = unsafe { EventCode::new(event_type, event.code) };

            match res {
                SUCCESS => events.push((event_code, event.value)),
                SYNC => {
                    events.push((event_code, event.value));
                    should_sync = true;
                },
                EAGAIN => break,
                _ => return Err(SystemError::new(
                    "An unknown error occured while reading from an event device."
                )),
            }
        }

        Ok(events)
    }

    /// Reads the raw events from the device and attached additional information such as the
    /// domain of this device and whatever value this event had the last time it was seen.
    fn _poll(&mut self) -> Result<Vec<Event>, SystemError> {
        let mut result: Vec<Event> = Vec::new();
        for (code, value) in self.read_raw()? {
            let previous_value_mut: &mut EventValue = self.state.entry(code).or_insert(0);
            let previous_value: EventValue = *previous_value_mut;
            *previous_value_mut = value;
            result.push(Event::new(
                code, value, previous_value, self.domain, Namespace::Input,
            ));
        }

        self.grab_if_desired()?;
        Ok(result)
    }

    fn grab_if_desired(&mut self) -> Result<(), SystemError> {
        if self.grabbed {
            return Ok(());
        }
        match self.grab_mode {
            GrabMode::None => Ok(()),
            GrabMode::Force => self.grab(),
            GrabMode::Auto => {
                // Grab if no key is currently pressed.
                for (event_code, value) in &self.state {
                    if event_code.ev_type().is_key() && *value > 0 {
                        return Ok(());
                    }
                }
                self.grab()
            }
        }
    }

    fn grab(&mut self) -> Result<(), SystemError> {
        let res = unsafe {
            libevdev::libevdev_grab(self.evdev, libevdev::libevdev_grab_mode_LIBEVDEV_GRAB)
        };
        if res < 0 {
            Err(SystemError::new(
                format!("Failed to grab event device: {}", self.path.to_string_lossy()
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

    // Closes the device and returns a blueprint from which it can be reopened.
    pub fn to_blueprint(&self) -> InputDeviceBlueprint {
        InputDeviceBlueprint {
            capabilities: self.capabilities.clone(),
            pre_device: PreInputDevice {
                path: self.path.clone(),
                grab_mode: self.grab_mode,
                domain: self.domain,
                persist_mode: self.persist_mode,
            }
        }
    }
}

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

impl Pollable for InputDevice {
    fn poll(&mut self) -> Result<Vec<Event>, Option<SystemError>> {
        self._poll().map_err(Option::Some)
    }

    fn reduce(self: Box<Self>) -> Result<Box<dyn Pollable>, Option<SystemError>> {
        eprintln!("The input device {} has been disconnected.", self.path.display()); // TODO: should this be moved?
        let blueprint = self.to_blueprint();
        match BlueprintOpener::new(blueprint) {
            Ok(opener) => Ok(Box::new(opener)),
            Err(error) => Err(Some(
                error.with_context("While trying to watch the reconnection of an input device:")
            )),
        }
    }
}

impl AsRawFd for InputDevice {
    fn as_raw_fd(&self) -> RawFd {
        self.file.as_raw_fd()
    }
}

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