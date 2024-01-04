// SPDX-License-Identifier: GPL-2.0-or-later

use std::ffi::{CStr, CString};
use std::fs::{File, OpenOptions};
use std::os::unix::fs::OpenOptionsExt;
use std::os::unix::io::{AsRawFd, RawFd};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::mem::MaybeUninit;
use crate::bindings::libevdev;
use crate::event::{Event, EventType, EventValue, EventCode, Namespace};
use crate::domain::Domain;
use crate::capability::{AbsInfo, Capabilities, InputCapabilites, RepeatInfo};
use crate::ecodes;
use crate::predevice::{GrabMode, PersistState, PreInputDevice};
use crate::persist::storage::CachedCapabilities;
use crate::error::{SystemError, Context};
use crate::persist::blueprint::Blueprint;
use crate::time::Instant;

use super::fd::HasFixedFd;

const ABOUT_CAPABILITIES_MSG: &str = "INFORMATION: Due to how the evdev protocol works, evsieve needs to declare exactly which events the virtual output devices can generate at the moment that those output devices are created. In order to do so, evsieve needs to know which events the input devices can generate. When \"persist\" or \"persist=full\" has been specified on an input device, evsieve will cache the capabilities of those input devices on the disk. If that input device is not present on a later run, evsieve will load those capabilities from the disk and use that information to decide which capabilities the output devices should have. When the input devices are not available and their capabilities have not been stored on the disk either, evsieve is not able to function properly. Please make sure that all input devices are present the first time you run a script.";

pub fn open_and_query_capabilities(pre_input_devices: Vec<PreInputDevice>)
    -> Result<(Vec<InputDevice>, Vec<Blueprint>, InputCapabilites), SystemError>
{
    let mut input_devices: Vec<InputDevice> = Vec::new();
    let mut blueprints: Vec<Blueprint> = Vec::new();
    
    for pre_device in pre_input_devices {
        match InputDevice::open(pre_device) {
            Ok(device) => {
                input_devices.push(device);
                continue;
            },
            Err((pre_device, error)) => {
                let error = error.with_context(format!("While opening the device \"{}\":", pre_device.path.display()));

                // Whether or failing to open an event device means that the whole operation fails depend on what
                // the specified persistence mode.
                match pre_device.persist_state {
                    // The following persistence modes tell us to exit if the device is not available when the program starts.
                    PersistState::None | PersistState::Reopen | PersistState::Exit => return Err(error),
                    // Full persistence tells us to try to find the capabilities of this device cached on the hard drive.
                    PersistState::Full(ref device_cache) => {
                        // TODO (High Priority): Do something about this "unknown" name
                        let unknown_name = CString::new("(unknown)").unwrap();

                        match device_cache.content {
                            // If we the capabilities of this device were properly cached, then we can just create a
                            // blueprint based on those capabilities.
                            CachedCapabilities::Known(ref capabilities_ref) => {
                                let capabilities = capabilities_ref.clone();
                                blueprints.push(Blueprint {
                                    pre_device,
                                    capabilities,
                                    name: unknown_name,
                                });
                            },
                            // If they are not found, then the best thing we could either exit with an error, or assume
                            // some arbitrary capabilities and carry on. I think that the latter option has the least
                            // chance of causing the user's system not to boot, and the name of the persistence mode is
                            // "full" after all, so...
                            CachedCapabilities::NonExistent => {
                                crate::utils::warn_once(ABOUT_CAPABILITIES_MSG);
                                eprintln!(
                                    "Error: the input device {} is not present, and its capabilities have not been stored on the disk either. Evsieve is unable to figure out which capabilities this device has. When this device is plugged in, evsieve will try to save its capabilities in the following file: {}",
                                    pre_device.path.display(),
                                    device_cache.location.display(),
                                );

                                // TODO (High Priority): This will cause evsieve to whine that an output device has been created to which no events can be routed.
                                blueprints.push(Blueprint { pre_device, capabilities: Capabilities::new(), name: unknown_name })
                            },
                            CachedCapabilities::Corrupted => {
                                eprintln!(
                                    "Error: the input device {} is not present, and its capabilities should have been stored on the disk, but evsieve is unable to interpret its file format. Maybe the content of the file have been corrupted. Evsieve will try to regenerate the file the next time the input device is seen, but in the meanwhile evsieve is unable to guess the capabilities of that input device, and thing will not work properly. The file in question is stored at: {}",
                                    pre_device.path.display(),
                                    device_cache.location.display(),
                                );

                                blueprints.push(Blueprint { pre_device, capabilities: Capabilities::new(), name: unknown_name })
                            },
                        }
                    },
                }
            }
        }
    }

    // Return an error if a device with grab=force cannot be grabbed.
    for device in &mut input_devices {
        device.grab_if_desired()?;
    }

    // Precompute the capabilities of the input devices.
    let mut capabilities: InputCapabilites = InputCapabilites::new();
    for device in &input_devices {
        // TODO: LOW-PRIORITY: Consider using an Rc instead of a clone.
        capabilities.insert(device.domain, device.capabilities.clone());
    }
    for blueprint in &blueprints {
        capabilities.insert(blueprint.pre_device.domain, blueprint.capabilities.clone());
    }

    Ok((input_devices, blueprints, capabilities))
}

/// Represents a name as reported by libevdev_get_name().
pub type InputDeviceName = CString;

pub struct InputDevice {
    /// The file owns the file descriptor to the input device. Beware: InputDevice implements HasFixedFd.
    file: File,
    inner: LibevdevDevice,

    /// The path to the input device that we opened.
    path: PathBuf,
    /// The evdev capabilities of the input device.
    capabilities: Capabilities,

    /// The name as reported by libevdev_get_name().
    name: InputDeviceName,

    /// Whether and how the user has requested this device to be grabbed. This may be different from whether
    /// it is actually grabbed at the present moment; that is being kept track of by `LibevdevDevice::grabbed`.
    grab_mode: GrabMode,

    /// The domain, though not part of libevdev, is a handy tag we use
    /// to track which device emitted the events.
    domain: Domain,

    /// Maps (type, code) pairs to the last known value of said pair.
    state: HashMap<EventCode, EventValue>,

    /// What should happen if this device disconnects.
    persist_state: PersistState,
}

/// This is a part of InputDevice that has been put in its separate structure to make working with destructors easier;
/// only this structure needs to implement Drop, which makes it possible to move things out of InputDevice.
pub struct LibevdevDevice {
    /// A pointer to the native libevdev structure.
    evdev: *mut libevdev::libevdev,

    /// Whether the device is actually grabbed.
    grabbed: bool,
}

impl InputDevice {
    /// Opens an input device from a given path.
    ///
    /// Does not grab the device even if grab=force is specified. You must do that manually later
    /// by calling grab_if_desired().
    /// 
    /// In case of error, returns the PreInputDevice back to the caller.
    #[allow(clippy::result_large_err)]
    pub fn open(pre_device: PreInputDevice) -> Result<InputDevice, (PreInputDevice, SystemError)> {
        // Open the file itself.
        let file_res = OpenOptions::new()
            .read(true)
            // O_CLOEXEC is already set by default in the std source code, but I'm providing it
            // anyway to clearly signify we _need_ that flag.
            .custom_flags(libc::O_NONBLOCK | libc::O_CLOEXEC)
            .open(&pre_device.path);

        let file = match file_res {
            Ok(file) => file,
            Err(error) => return Err((pre_device, error.into())),
        };

        // Turn the file into an evdev instance.
        let mut evdev: *mut libevdev::libevdev = std::ptr::null_mut();
        let res = unsafe {
            libevdev::libevdev_new_from_fd(file.as_raw_fd(), &mut evdev)
        };
        if res < 0 {
            let error_msg = format!("Failed to open a libevdev instance: {}.", pre_device.path.to_string_lossy());
            return Err((pre_device, SystemError::new(error_msg)));
        }

        let capabilities = unsafe { get_capabilities(evdev) };
        let state = unsafe { get_device_state(evdev, &capabilities) };

        // According to the documentation, libevdev_get_name() never returns a null pointer
        // but may return an empty string. We are not sure whether the return value is guaranteed
        // to be UTF-8 decodable, so it may be possible that device_name ends up as None.
        let name: InputDeviceName = unsafe {
            CStr::from_ptr(libevdev::libevdev_get_name(evdev))
        }.to_owned();

        // Set the clock to CLOCK_MONOTONIC, which is the same clock used for all other time-
        // related operations used in evsieve. Using the monotonic clock instead of the
        // realtime clock makes sure that some event will not end up getting delayed by many
        // days in case the user decides to set their clock back.
        //
        // The libevdev documentation says that "This is a modification only affecting this
        // representation of this device."; setting this clock id should not affect other programs.
        let res = unsafe { libevdev::libevdev_set_clock_id(evdev, libc::CLOCK_MONOTONIC) };
        if res < 0 {
            eprintln!("Warning: failed to set the clock to CLOCK_MONOTONIC on the device opened from {}.\nThis is a non-fatal error, but any time-related operations such as the --delay argument will behave incorrectly.", pre_device.path.to_string_lossy());
        }

        // Now that we know the real input capabilities of this device, update the cache on the
        // disk if necessary.
        let mut persist_state = pre_device.persist_state;
        persist_state.update_caps(&capabilities, &pre_device.path);

        Ok(InputDevice {
            file, capabilities, state, name,
            path: pre_device.path,
            domain: pre_device.domain,
            grab_mode: pre_device.grab_mode,
            persist_state,
            inner: LibevdevDevice {
                evdev, grabbed: false
            }
        })
    }

    pub fn domain(&self) -> Domain {
        self.domain
    }

    fn read_raw(&mut self) -> Result<Vec<(Instant, EventCode, EventValue)>, SystemError> {
        let mut event: MaybeUninit<libevdev::input_event> = MaybeUninit::uninit();
        let mut should_sync = false;
        let mut events: Vec<(Instant, EventCode, EventValue)> = Vec::new();

        loop {
            let flags = match should_sync {
                true => libevdev::libevdev_read_flag_LIBEVDEV_READ_FLAG_SYNC,
                false => libevdev::libevdev_read_flag_LIBEVDEV_READ_FLAG_NORMAL,
            };
            let res = unsafe {
                libevdev::libevdev_next_event(self.inner.evdev, flags, event.as_mut_ptr())
            };

            const SUCCESS: i32 = libevdev::libevdev_read_status_LIBEVDEV_READ_STATUS_SUCCESS as i32;
            const SYNC: i32 = libevdev::libevdev_read_status_LIBEVDEV_READ_STATUS_SYNC as i32;
            const MINUS_EAGAIN: i32 = -libc::EAGAIN;
            const MINUS_EINTR: i32 = -libc::EINTR;

            match res {
                SUCCESS | SYNC => {
                    unsafe {
                        let event = event.assume_init();
                        let event_type = EventType::new(event.type_);
                        let event_code = EventCode::new(event_type, event.code);
                        let event_time = event.time.into();
                        events.push((event_time, event_code, event.value));
                    }

                    should_sync = res == SYNC;
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
    pub fn poll(&mut self) -> Result<Vec<(Instant, Event)>, SystemError> {
        let events: Vec<(Instant, Event)> = self.read_raw()?
            .into_iter()
            .map(|(time, code, value)| (time, self.synthesize_event(code, value)))
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
        if self.inner.grabbed {
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

    fn grab(&mut self) -> Result<(), SystemError> {
        self.inner.grab().with_context_of(|| format!("While trying to grab {}:", self.path.display()))
    }

    /// Returns an iterator of all EV_KEY codes that are currently pressed.
    pub fn get_pressed_keys(&self) -> impl Iterator<Item=EventCode> + '_ {
        self.state.iter()
            .filter(|(code, value)| code.ev_type().is_key() && **value > 0)
            .map(|(&code, &_value)| code)
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

    pub fn persist_state(&self) -> &PersistState {
        &self.persist_state
    }

    // Closes the device and returns a blueprint from which it can be reopened.
    pub fn into_blueprint(self) -> Blueprint {
        Blueprint {
            capabilities: self.capabilities,
            name: self.name,
            pre_device: PreInputDevice {
                path: self.path,
                grab_mode: self.grab_mode,
                domain: self.domain,
                persist_state: self.persist_state,
            },
        }
    }
}

impl LibevdevDevice { 
    fn grab(&mut self) -> Result<(), SystemError> {
        let res = unsafe {
            libevdev::libevdev_grab(self.evdev, libevdev::libevdev_grab_mode_LIBEVDEV_GRAB)
        };
        if res < 0 {
            Err(SystemError::new(
                format!("Failed to grab input device: received libevdev status code {res}"
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
                format!("Failed to ungrab input device: received libevdev status code {res}"
            )))
        } else {
            self.grabbed = false;
            Ok(())
        }
    }
}

/// This implement is necessary becaus *mut libevdev::libevdev is not Send.
unsafe impl Send for InputDevice {}

/// # Safety
/// Exhibits undefined behaviour if evdev is not a valid pointer.
unsafe fn get_capabilities(evdev: *mut libevdev::libevdev) -> Capabilities {
    let event_types = ecodes::event_types();
    
    let supported_event_types = event_types.filter(|&ev_type| {
        libevdev::libevdev_has_event_type(evdev, ev_type.into()) == 1
    });

    let supported_event_codes: HashSet<EventCode> =
        supported_event_types
        .flat_map(ecodes::event_codes_for)
        .filter(|code| {
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
        let res = libevdev::libevdev_get_repeat(evdev, &mut delay, &mut period);
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

impl Drop for LibevdevDevice {
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
