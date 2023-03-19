// SPDX-License-Identifier: GPL-2.0-or-later

use std::fmt::Error;
use std::io;
use std::fs;
use std::ffi::{CString};
use std::ptr;
use std::collections::HashMap;
use std::path::{Path};
use std::path::PathBuf;
use std::fmt::Write;
use crate::event::EventType;
use crate::bindings::libevdev;
use crate::capability::{Capability, Capabilities};
use crate::event::Event;
use crate::domain::Domain;
use crate::ecodes;
use crate::error::{InternalError, RuntimeError, SystemError, Context};
use crate::predevice::{PreOutputDevice, RepeatMode};

pub struct OutputSystem {
    pre_devices: Vec<PreOutputDevice>,
    devices: HashMap<Domain, OutputDevice>,
}

impl OutputSystem {
    pub fn create(
            pre_devices: Vec<PreOutputDevice>,
            capabilities: Vec<Capability>
    ) -> Result<OutputSystem, RuntimeError> {
        // Sort the capabilities based on domain.
        let mut capability_map = capabilites_by_device(&capabilities, &pre_devices);

        // Create domains with capabilities.
        let mut devices: HashMap<Domain, OutputDevice> = HashMap::new();
        for pre_device in &pre_devices {
            let domain = pre_device.domain;

            if devices.contains_key(&domain) {
                return Err(InternalError::new("Multiple output devices with the same domain have been created.").into());
            }
    
            let capabilities = capability_map.remove(&pre_device.domain).expect("Internal invariant violated: capabilites_by_device() did not create a capability entry for each output device.");
            if capabilities.has_no_content() {
                eprintln!("Warning: an output device has been specified to which no events can possibly be routed.");
            }

            let device = create_output_device(pre_device, capabilities)?;
            
            devices.insert(domain, device);
        }

        Ok(OutputSystem { pre_devices, devices })
    }

    /// Tries to make sure that all output devices have at least the given capabilities. The output 
    /// devices may or may not end up with more capabilities than specified.
    ///
    /// This may cause output devices to be destroyed and recreated.
    pub fn update_caps(&mut self, new_capabilities: Vec<Capability>) {
        // Sort the capabilities based on domain.
        let mut capability_map = capabilites_by_device(&new_capabilities, &self.pre_devices);

        let old_output_devices = std::mem::take(&mut self.devices);
        let mut recreated_output_devices: Vec<&PreOutputDevice> = Vec::new();

        for (domain, mut old_device) in old_output_devices {
            // Find the new capabilities for this domain.
            let capabilities = match capability_map.remove(&domain) {
                Some(caps) => caps,
                None => {
                    eprintln!("Internal invariant violated: capability_map does not contain capabilities for all output devices. This is a bug.");
                    self.devices.insert(domain, old_device);
                    continue;
                },
            };

            // Find the pre_output_device with the same domain.
            let pre_device: &PreOutputDevice = match self.pre_devices.iter().find(
                |pre_device| pre_device.domain == domain
            ) {
                Some(pre_device) => pre_device,
                None => {
                    eprintln!("Internal invariant violated: OutputDeviceSystem contains an output device with a domain for which it does not have a PreOutputDevice. This is a bug.");
                    self.devices.insert(domain, old_device);
                    continue;
                },
            };

            if capabilities.is_compatible_with(&old_device.capabilities) {
                self.devices.insert(domain, old_device);
                continue;
            }

            // The device is supposed to have more capabilities than it used to. We must recreate it.
            // Free up the old symlink so the new device can create a symlink in its place.
            let symlink = old_device.take_symlink();
            drop(symlink); // TODO: MEDIUM-PRIORITY: make this operation atomical with its recreation.

            let new_device = match create_output_device(pre_device, capabilities) {
                Ok(device) => device,
                Err(error) => {
                    eprintln!("Error: failed to recreate an output device. The remaining output devices may have incorrect capabilities.");
                    error.print_err();
                    // Try to restore the old link if possible.
                    if let Some(ref path) = pre_device.create_link {
                        old_device.set_link(path.clone()).print_err();
                    }
                    self.devices.insert(domain, old_device);
                    continue;
                }
            };

            old_device.syn_if_required();
            drop(old_device);

            self.devices.insert(domain, new_device);
            recreated_output_devices.push(pre_device);
        }

        if ! recreated_output_devices.is_empty() {
            if let Ok(warning_msg) = format_output_device_recreation_warning(&recreated_output_devices) {
                println!("{}", warning_msg);
            } else {
                println!("Warning: output devices have been recreated.");
                eprintln!("Internal error: an unknown error occured in our error formatting logic. This is a bug.");
            }
        }
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
    /// The capabilities of this output device.
    capabilities: Capabilities,
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

            // If EV_MSC events are automatically generated, we may need to manually activate
            // their capabilities.
            // TODO: FEATURE(auto-scan) prevent EV_MSC capabilities from getting activated by
            // capabilities.
            // ... in fact, refactor this to cooperate with the capability interface.
            if cfg!(feature = "auto-scan") {
                libevdev::libevdev_enable_event_type(dev, EventType::MSC.into());
                libevdev::libevdev_enable_event_code(
                    dev,
                    EventType::MSC.into(),
                    crate::event::EventCode::MSC_SCAN.code() as u32,
                    ptr::null_mut()
                );
            }

            for &ev_type in &caps.ev_types() {
                libevdev::libevdev_enable_event_type(dev, ev_type.into());
            }
            for code in &caps.codes {
                let res = match code.ev_type() {
                    EventType::ABS => {
                        let abs_info = caps.abs_info.get(code)
                            .ok_or_else(|| InternalError::new("Cannot create uinput device: device has absolute axis without associated capabilities."))?;
                        let libevdev_abs_info: libevdev::input_absinfo = (*abs_info).into();
                        let libevdev_abs_info_ptr = &libevdev_abs_info as *const libevdev::input_absinfo;
                        libevdev::libevdev_enable_event_code(
                            dev, code.ev_type().into(), code.code() as u32, libevdev_abs_info_ptr as *const libc::c_void)
                    },
                    EventType::REP => {
                        // Known issue: due to limitations in the uinput kernel module, the REP_DELAY
                        // and REP_PERIOD values are ignored and the kernel defaults will be used instead,
                        // according to the libevdev documentation. Status: won't fix.
                        if let Some(rep_info) = caps.rep_info {
                            let value: libc::c_int = match code.code() {
                                ecodes::REP_DELAY => rep_info.delay,
                                ecodes::REP_PERIOD => rep_info.period,
                                _ => {
                                    eprintln!("Warning: encountered an unknown capability code under EV_REP: {}.", code.code());
                                    continue;
                                },
                            };
                            libevdev::libevdev_enable_event_code(dev, code.ev_type().into(), code.code() as u32, &value as *const libc::c_int as *const libc::c_void)
                        } else {
                            eprintln!("Internal error: an output device claims EV_REP capabilities, but no repeat info is available.");
                            continue;
                        }
                    },
                    _ => libevdev::libevdev_enable_event_code(dev, code.ev_type().into(), code.code() as u32, ptr::null_mut()),
                };
                if res < 0 {
                    eprintln!("Warning: failed to enable event {} on uinput device.", ecodes::event_name(*code));
                }
            }

            let mut uinput_dev: *mut libevdev::libevdev_uinput = ptr::null_mut();
            let res = libevdev::libevdev_uinput_create_from_device(
                dev,
                // In the source code of the current version of libevdev, the O_CLOEXEC will be
                // automatically set on the created file descriptor.
                libevdev::libevdev_uinput_open_mode_LIBEVDEV_UINPUT_OPEN_MANAGED,
                &mut uinput_dev
            );

            // After we've created an UInput device based on this, we no longer need the original prototype.
            libevdev::libevdev_free(dev);

            if res != 0 {
                return Err(SystemError::new("Failed to create an UInput device. Does evsieve have enough permissions?").into());
            }

            Ok(OutputDevice {
                device: uinput_dev,
                should_syn: false,
                symlink: None,
                allows_repeat: true,
                capabilities: caps,
            })
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
        self.should_syn = ev_type != libevdev::EV_SYN;
    }

    #[cfg(not(feature = "auto-scan"))]
    fn write_event(&mut self, event: Event) {
        self.write(event.code.ev_type().into(), event.code.code() as u32, event.value);
    }

    #[cfg(feature = "auto-scan")]
    fn write_event(&mut self, event: Event) {
        // TODO: LOW-PRIORITY conside moving the following snippet to another stage of the event pipeline.
        if event.ev_type() == EventType::KEY && (event.value == 0 || event.value == 1) {
            if let Some(scancode) = crate::scancodes::from_event_code(event.code) {
                self.write(EventType::MSC.into(), crate::event::EventCode::MSC_SCAN.code().into(), scancode)
            }
        }
        self.write(event.code.ev_type().into(), event.code.code() as u32, event.value as i32);
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
        if my_path_cstr_ptr.is_null() {
            return Err(SystemError::new("Failed to createa a symlink to an output device: cannot determine the path to the virtual device's device node."))
        };
        let my_path_cstr = unsafe { std::ffi::CStr::from_ptr(my_path_cstr_ptr) };
        let my_path_str = my_path_cstr.to_str().map_err(|_|
            SystemError::new("Failed to createa a symlink to an output device: the path to the virtual device node is not valid UTF-8.")
        )?;
        let my_path = Path::new(my_path_str).to_owned();

        // Drop the old link before creating a new one, in case the old and new link are both at the
        // same location.
        drop(self.take_symlink());
        self.symlink = Some(Symlink::create(my_path, path)?);
        Ok(())
    }

    /// Decouples this device from the symlink pointing to it.
    fn take_symlink(&mut self) -> Option<Symlink> {
        self.symlink.take()
    }

    fn set_repeat_mode(&mut self, mode: RepeatMode) {
        self.allow_repeat(match mode {
            RepeatMode::Passive  => true,
            RepeatMode::Disable  => false,
            RepeatMode::Enable   => false,
        });
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

/// Sorts a vector of capabilities by domain and adjusts them based on explicit flags/clauses specified
/// for the output devices. Guarantees that an entry exists for each pre-output device.
fn capabilites_by_device(capabilities: &[Capability], pre_devices: &[PreOutputDevice])
        -> HashMap<Domain, Capabilities>
{
    let mut capability_map: HashMap<Domain, Capabilities> = HashMap::new();
    for capability in capabilities.iter().copied() {
        let domain_capabilities = capability_map.entry(capability.domain).or_insert_with(Capabilities::new);
        domain_capabilities.add_capability(capability);
    }

    for device in pre_devices {
        let device_caps = capability_map.entry(device.domain).or_insert_with(Capabilities::new);
        match device.repeat_mode {
            RepeatMode::Disable => device_caps.remove_ev_rep(),
            RepeatMode::Passive => device_caps.remove_ev_rep(),
            RepeatMode::Enable  => device_caps.require_ev_rep(),
        };
    }

    capability_map
}

fn create_output_device(pre_device: &PreOutputDevice, capabilities: Capabilities) -> Result<OutputDevice, RuntimeError> {
    let mut device = OutputDevice::with_name_and_capabilities(pre_device.name.clone(), capabilities)
        .with_context(match pre_device.create_link.clone() {
            Some(path) => format!("While creating the output device \"{}\":", path.display()),
            None => "While creating an output device:".to_string(),
        })?;

    device.set_repeat_mode(pre_device.repeat_mode);

    if let Some(ref path) = pre_device.create_link {
        device.set_link(path.clone())
            .map_err(SystemError::from)
            .with_context(format!("While creating a symlink at \"{}\":", path.display()))?;
    };

    Ok(device)
}

fn format_output_device_recreation_warning(recreated_devices: &[&PreOutputDevice]) -> Result<String, Error>  {
    if recreated_devices.is_empty() {
        return Ok("".to_owned());
    }
    let named_recreated_devices: Vec<String> = recreated_devices.iter().filter_map(
        |device| device.create_link.as_ref().map(
            |path| format!("\"{}\"", path.display())
        )
    ).collect();
    let num_unnamed_recreated_devices = recreated_devices.len() - named_recreated_devices.len();

    let mut msg: String = "Warning: due to a change in the capabilities of the input devices, ".to_string();
    if named_recreated_devices.is_empty() {
        match num_unnamed_recreated_devices {
            1 => write!(&mut msg, "an output device ")?,
            n => write!(&mut msg, "{} output devices ", n)?,
        };
    } else {
        match named_recreated_devices.len() {
            1 => write!(&mut msg, "the output device ")?,
            _ => write!(&mut msg, "the output devices ")?,
        };
        write!(&mut msg, "{}", named_recreated_devices.join(", "))?;
        match num_unnamed_recreated_devices {
            0 => write!(&mut msg, " ")?,
            1 => write!(&mut msg, ", and an unnamed output device ")?,
            n => write!(&mut msg, ", and {} unnamed output devices ", n)?,
        };
    };

    match recreated_devices.len() {
        1 => write!(&mut msg, "has been destroyed and recreated. ")?,
        _ => write!(&mut msg, "have been destroyed and recreated. ")?,
    }
    write!(&mut msg, "This may cause other programs that have grabbed the output devices to lose track of them.")?;

    Ok(msg)
}