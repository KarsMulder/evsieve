// SPDX-License-Identifier: GPL-2.0-or-later

use std::io;
use std::fs;
use std::ffi::{CString};
use std::ptr;
use std::collections::HashMap;
use std::path::{Path};
use std::path::PathBuf;
use crate::event::EventType;
use crate::bindings::libevdev;
use crate::capability::{Capability, Capabilities};
use crate::event::Event;
use crate::domain::Domain;
use crate::ecodes;
use crate::error::{InternalError, RuntimeError, SystemError, Context};
use crate::predevice::{PreOutputDevice, RepeatMode};

pub struct OutputSystem {
    devices: HashMap<Domain, OutputDevice>,
}

impl OutputSystem {
    pub fn create(
            pre_devices: Vec<PreOutputDevice>,
            capabilities: Vec<Capability>
    ) -> Result<OutputSystem, RuntimeError> {
        // Sort the capabilities based on domain.
        let mut capability_map: HashMap<Domain, Capabilities> = HashMap::new();
        for capability in capabilities {
            let domain_capabilities = capability_map.entry(capability.domain).or_insert_with(Capabilities::new);
            domain_capabilities.add_capability(capability);
        }

        // Create domains with capabilities.
        let mut devices: HashMap<Domain, OutputDevice> = HashMap::new();
        for pre_device in pre_devices {
            let domain = pre_device.domain;

            if devices.contains_key(&domain) {
                return Err(InternalError::new("Multiple output devices with the same domain have been created.").into());
            }

            let mut capabilities = capability_map.get(&domain).cloned().unwrap_or_else(Capabilities::new);
            match pre_device.repeat_mode {
                RepeatMode::Disable => capabilities.remove_ev_rep(),
                RepeatMode::Passive => capabilities.remove_ev_rep(),
                RepeatMode::Enable  => capabilities.require_ev_rep(),
            };
            if capabilities.is_empty() {
                eprintln!("Warning: an output device has been specified to which no events can possibly be routed.");
            }

            let mut device = OutputDevice::with_name_and_capabilities(pre_device.name.clone(), capabilities)
                .with_context(match pre_device.create_link.clone() {
                    Some(path) => format!("While creating the output device \"{}\":", path.display()),
                    None => "While creating an output device:".to_string(),
                })?;

            device.allow_repeat(match pre_device.repeat_mode {
                RepeatMode::Passive  => true,
                RepeatMode::Disable  => false,
                RepeatMode::Enable   => false,
            });
            if let Some(path) = pre_device.create_link {
                device.set_link(path.clone())
                    .map_err(SystemError::from)
                    .with_context(format!("While creating a symlink at \"{}\":", path.display()))?;
            };
            
            devices.insert(domain, device);
        }

        Ok(OutputSystem { devices })
    }

    /// Writes all events to their respective output devices.
    pub fn route_events(&mut self, events: &[Event]) {
        for &event in events {
            let device_opt = self.devices.get_mut(&event.domain);
            match device_opt {
                Some(device) => device.write_event(event),
                None => eprintln!("Internal error: an event {} with unknown domain has been routed to output; event dropped. This is a bug.", event),
            };
        }
    }

    /// The maps may generate events without folling them up with SYN events.
    /// This function generates all SYN events for user convenience.
    pub fn synchronize(&mut self) {
        for device in self.devices.values_mut() {
            device.syn_if_required();
        }
    }
}

pub struct OutputDevice {
    device: *mut libevdev::libevdev_uinput,
    /// Keeps track of whether we've sent any events to the output since the last SYN event.
    should_syn: bool,
    /// If some symlink to the device was created, store it here.
    symlink: Option<Symlink>,
    /// If false, all repeat events sent to this device will be dropped.
    /// Does not prevent the kernel from generating repeat events.
    allows_repeat: bool,
}

impl OutputDevice {
    pub fn with_name_and_capabilities(name_str: String, caps: Capabilities) -> Result<OutputDevice, RuntimeError> {
        unsafe {
            let dev = libevdev::libevdev_new();

            let cstr = CString::new(name_str).unwrap();
            let bytes = cstr.as_bytes_with_nul();
            let ptr = bytes.as_ptr();
            let name = ptr as *const libc::c_char;

            libevdev::libevdev_set_name(dev, name);

            for &ev_type in &caps.ev_types {
                libevdev::libevdev_enable_event_type(dev, ev_type.into());
            }
            for &(ev_type, code) in &caps.codes {
                let res = match ev_type {
                    EventType::ABS => {
                        let abs_info = caps.abs_info.get(&(ev_type, code))
                            .ok_or_else(|| InternalError::new("Cannot create uinput device: device has absolute axis without associated capabilities."))?;
                        let libevdev_abs_info: libevdev::input_absinfo = (*abs_info).into();
                        let libevdev_abs_info_ptr = &libevdev_abs_info as *const libevdev::input_absinfo;
                        libevdev::libevdev_enable_event_code(
                            dev, ev_type.into(), code as u32, libevdev_abs_info_ptr as *const libc::c_void)
                    },
                    EventType::REP => {
                        // Known issue: due to limitations in the uinput kernel module, the REP_DELAY
                        // and REP_PERIOD values are ignored and the kernel defaults will be used instead,
                        // according to the libevdev documentation. Status: won't fix.
                        if let Some(rep_info) = caps.rep_info {
                            let value: libc::c_int = match code {
                                ecodes::REP_DELAY => rep_info.delay,
                                ecodes::REP_PERIOD => rep_info.period,
                                _ => {
                                    eprintln!("Warning: encountered an unknown capability code under EV_REP: {}.", code);
                                    continue;
                                },
                            };
                            libevdev::libevdev_enable_event_code(dev, ev_type.into(), code as u32, &value as *const libc::c_int as *const libc::c_void)
                        } else {
                            eprintln!("Internal error: an output device claims EV_REP capabilities, but no repeat info is available.");
                            continue;
                        }
                    },
                    _ => libevdev::libevdev_enable_event_code(dev, ev_type.into(), code as u32, ptr::null_mut()),
                };
                if res < 0 {
                    eprintln!("Warning: failed to enable event {} on uinput device.", ecodes::event_name(ev_type, code));
                }
            }

            let mut uinput_dev: *mut libevdev::libevdev_uinput = ptr::null_mut();
            let res = libevdev::libevdev_uinput_create_from_device(
                dev,
                libevdev::libevdev_uinput_open_mode_LIBEVDEV_UINPUT_OPEN_MANAGED,
                &mut uinput_dev
            );

            // After we've created an UInput device based on this, we no longer need the original prototype.
            libevdev::libevdev_free(dev);

            if res != 0 {
                return Err(SystemError::new("Failed to create an UInput device. Does evsieve have enough permissions?").into());
            }

            Ok(OutputDevice { device: uinput_dev, should_syn: false, symlink: None, allows_repeat: true })
        }
    }

    fn write(&mut self, ev_type: u32, code: u32, value: i32) {
        if ! self.allows_repeat && ev_type == ecodes::EV_KEY.into() && value == 2 {
            return;
        }
        let res = unsafe { libevdev::libevdev_uinput_write_event(self.device, ev_type, code, value) };
        if res < 0 {
            eprintln!("Warning: an error occurred while writing an event to {}.", self.description());
        }
        self.should_syn = ev_type as u32 != libevdev::EV_SYN;
    }

    fn write_event(&mut self, event: Event) {
        self.write(event.ev_type.into(), event.code as u32, event.value as i32);
    }

    fn syn_if_required(&mut self) {
        if self.should_syn {
            self.write(libevdev::EV_SYN, 0, 0);
        }
    }

    /// Returns a handy name for this device, useful for error logging.main
    fn description(&self) -> String {
        if let Some(link) = &self.symlink {
            format!("the output device \"{}\"", link.location().to_string_lossy())
        } else {
            "an output device".to_string()
        }
    }

    fn set_link(&mut self, path: PathBuf) -> Result<(), SystemError> {
        // Try to figure out the path of the uinput device node.
        let my_path_cstr_ptr = unsafe {
            libevdev::libevdev_uinput_get_devnode(self.device)
        };
        if my_path_cstr_ptr == std::ptr::null() {
            return Err(SystemError::new("Failed to createa a symlink to an output device: cannot determine the path to the virtual device's device node."))
        };
        let my_path_cstr = unsafe { std::ffi::CStr::from_ptr(my_path_cstr_ptr) };
        let my_path_str = my_path_cstr.to_str().map_err(|_|
            SystemError::new("Failed to createa a symlink to an output device: the path to the virtual device node is not valid UTF-8.")
        )?;
        let my_path = Path::new(my_path_str).to_owned();

        // Creaet the actual link.
        self.symlink = Some(Symlink::create(my_path, path)?);
        Ok(())
    }

    fn allow_repeat(&mut self, value: bool) {
        self.allows_repeat = value;
    }
}

impl Drop for OutputDevice {
    fn drop(&mut self) {
        unsafe {
            libevdev::libevdev_uinput_destroy(self.device);
        }
    }
}

/// Represents a symlink on the filesystem. Has RAII support.
struct Symlink {
    /// Where the symlink points to.
    _source: PathBuf,
    /// Where the actual symlink lives.
    location: PathBuf,
}

impl Symlink {
    /// Creates a symlink at dest that points to source.
    fn create(source: PathBuf, dest: PathBuf) -> Result<Symlink, io::Error> {
        // Handle already existing files: overwrite if it exists and is a symlink,
        // otherwise leave untouched. This behaviour has been chosen as the optimal
        // balance between being nondestructive yet also not confusing the user with
        // errors after re-running a script that has been SIGKILL'd.
        if let Ok(metadata) = fs::symlink_metadata(&dest) {
            if metadata.file_type().is_symlink() {
                fs::remove_file(&dest)?;
            } else {
                return Err(io::Error::new(io::ErrorKind::AlreadyExists,
                    format!("Cannot create a symlink at \"{}\": path already exists.", dest.to_string_lossy())));
            }
        }

        std::os::unix::fs::symlink(&source, &dest)?;
        Ok(Symlink {
            _source: source, location: dest
        })
    }

    fn location(&self) -> &Path {
        &self.location
    }
}

impl Drop for Symlink {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.location);
    }
}
