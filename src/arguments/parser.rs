// SPDX-License-Identifier: GPL-2.0-or-later

use crate::domain;
use crate::error::{ArgumentError, RuntimeError, Context};
use crate::key::Key;
use crate::event::Namespace;
use crate::hook::Hook;
use crate::map::{Map, Toggle};
use crate::stream::{StreamEntry, Setup};
use crate::predevice::{PreInputDevice, PreOutputDevice};
use crate::state::{State, ToggleIndex};
use crate::arguments::hook::HookArg;
use crate::arguments::input::InputDevice;
use crate::arguments::output::OutputDevice;
use crate::arguments::toggle::ToggleArg;
use crate::arguments::map::{MapArg, BlockArg};
use crate::arguments::print::PrintArg;
use crate::arguments::delay::DelayArg;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use super::merge::MergeArg;

const VERSION: Option<&'static str> = option_env!("CARGO_PKG_VERSION");
const USAGE_MSG: &str = 
"Usage: evsieve [--input PATH... [domain=DOMAIN] [grab[=auto|force]] [persist=reopen|none]]...
               [--map SOURCE [DEST...] [yield]]...
               [--copy SOURCE [DEST...] [yield]]...
               [--block [SOURCE...]]...
               [--toggle SOURCE DEST... [id=ID] [mode=consistent|passive]]...
               [--hook KEY... [exec-shell=COMMAND]... [toggle[=[ID][:INDEX]]]... [period=SECONDS]]...
               [--merge [EVENTS...]]...
               [--print [EVENTS...] [format=default|direct]]...
               [--delay [EVENTS...] period=SECONDS]...
               [--output [EVENTS...] [create-link=PATH] [name=NAME] [repeat[=MODE]]]...";

enum Argument {
    InputDevice(InputDevice),
    OutputDevice(OutputDevice),
    MapArg(MapArg),
    HookArg(HookArg),
    BlockArg(BlockArg),
    ToggleArg(ToggleArg),
    PrintArg(PrintArg),
    MergeArg(MergeArg),
    DelayArg(DelayArg),
}

impl Argument {
    fn parse(args: Vec<String>) -> Result<Argument, RuntimeError> {
        let first_arg = args[0].clone();
        match first_arg.as_str() {
            "--input" => Ok(Argument::InputDevice(InputDevice::parse(args)?)),
            "--output" => Ok(Argument::OutputDevice(OutputDevice::parse(args)?)),
            "--map" => Ok(Argument::MapArg(MapArg::parse(args)?)),
            "--copy" => Ok(Argument::MapArg(MapArg::parse(args)?)),
            "--hook" => Ok(Argument::HookArg(HookArg::parse(args)?)),
            "--toggle" => Ok(Argument::ToggleArg(ToggleArg::parse(args)?)),
            "--block" => Ok(Argument::BlockArg(BlockArg::parse(args)?)),
            "--print" => Ok(Argument::PrintArg(PrintArg::parse(args)?)),
            "--merge" => Ok(Argument::MergeArg(MergeArg::parse(args)?)),
            "--delay" => Ok(Argument::DelayArg(DelayArg::parse(args)?)),
            _ => Err(ArgumentError::new(format!("Encountered unknown argument: {}", first_arg)).into()),
        }
    }
}

/// If a --version or --help or something is specified, prints a helpful message.
/// Returns true if --version or --help was requested, otherwise returns false.
pub fn check_help_and_version(args: &[String]) -> bool {
    if args.len() == 1 // Only the program name.
            || args.contains(&"-?".to_owned())
            || args.contains(&"-h".to_owned())
            || args.contains(&"--help".to_owned()) {
        println!("{}", USAGE_MSG);
        return true;
    }

    if args.contains(&"--version".to_owned()) {
        let version = VERSION.unwrap_or("unknown");
        println!("{}", version);
        return true;
    }

    false
}

fn parse(args: Vec<String>) -> Result<Vec<Argument>, RuntimeError> {
	// Sort the arguments into groups.
    let mut groups: Vec<Vec<String>> = Vec::new();
    let mut args_iter = args.into_iter().peekable();
    args_iter.next(); // Skip the program name.
	while let Some(first_arg) = args_iter.next() {
		if ! first_arg.starts_with("--") {
			return Err(ArgumentError::new(format!(
                "Expected an argument starting with --, encountered \"{}\".", first_arg
            )).into());
        }

        // Take items from the arg list until we encounter the start of the next argument.
        let mut new_group: Vec<String> = vec![first_arg];
        while let Some(next_arg) = args_iter.peek() {
            if next_arg.starts_with("--") {
                break;
            }
            new_group.push(args_iter.next().unwrap());
        }
		
		groups.push(new_group);
    }

    groups.into_iter().map(
        |group| Argument::parse(group.clone()).with_context(format!(
            "While parsing the arguments \"{}\":", group.join(" ")
        )
    )).collect::<Result<Vec<Argument>, RuntimeError>>()
}

pub struct Implementation {
    pub setup: Setup,
    pub input_devices: Vec<crate::io::input::InputDevice>,
}

/// This function does most of the work of turning the input arguments into the components of a
/// runnable program.
pub fn implement(args_str: Vec<String>)
        -> Result<Implementation, RuntimeError>
{
    let args: Vec<Argument> = parse(args_str)?;
    let mut input_devices: Vec<PreInputDevice> = Vec::new();
    let mut output_devices: Vec<PreOutputDevice> = Vec::new();
    let mut stream: Vec<StreamEntry> = Vec::new();

    let mut state: State = State::new();

    // Maps a toggle's ID to the index at which it can be found.
    let mut toggle_indices: HashMap<String, ToggleIndex> = HashMap::new();

    // Reserve toggle indices ahead of time so --hooks can act upon indices of toggles
    // that will only be defined later.
    for arg in &args {
        if let Argument::ToggleArg(toggle_arg) = arg {
            if let Some(id) = toggle_arg.id.clone() {
                match toggle_indices.get(&id) {
                    Some(_) => {
                        return Err(ArgumentError::new("Two toggles cannot have the same id.").into());
                    },
                    None => {
                        let index = state.create_toggle_with_size(toggle_arg.size())?;
                        toggle_indices.insert(id, index);
                    }
                }
            }
        }
    }

    // Keep trach of the real paths for the input devices we've opened so we don't open the same
    // one twice.
    let mut input_device_real_paths: HashSet<PathBuf> = HashSet::new();

    // Construct the stream.
    for arg in args {
        match arg {
            Argument::InputDevice(device) => {
                for path_str in &device.paths {
                    let path: PathBuf = path_str.into();
                    let real_path = std::fs::canonicalize(path.clone()).map_err(
                        |_| ArgumentError::new(format!("The input device \"{}\" does not exist.", path_str))
                    )?;

                    // Opening the same device multiple times could spell trouble for certain
                    // possible future features and has little purpose, so we don't allow it.
                    if input_device_real_paths.contains(&real_path) {
                        return Err(ArgumentError::new(format!("The input device \"{}\" has been opened multiple times.", path_str)).into());
                    } else {
                        input_device_real_paths.insert(real_path);
                    }
                    
                    let source_domain = domain::get_unique_domain();
                    let target_domain = match &device.domain {
                        Some(value) => *value,
                        None => domain::resolve(path_str)?,
                    };

                    let input_device = PreInputDevice {
                        path, domain: source_domain,
                        grab_mode: device.grab_mode,
                        persist_mode: device.persist_mode,
                    };

                    // Register this device for later creation.
                    input_devices.push(input_device);
                    // Create a map to put those events into the stream at the right time.
                    stream.push(StreamEntry::Map(
                        Map::domain_shift(
                            source_domain, Namespace::Input,
                            target_domain, Namespace::User,
                        )
                    ));
                }
            },
            Argument::OutputDevice(device) => {
                // Create the output device.
                let target_domain = domain::get_unique_domain();
                let output_device = PreOutputDevice {
                    domain: target_domain,
                    create_link: device.create_link,
                    name: device.name,
                    repeat_mode: device.repeat_mode,
                };
                output_devices.push(output_device);
                
                // Map the keys to this output device.
                for key in device.keys {
                    let map = Map::new(
                        key,
                        vec![Key::from_domain_and_namespace(target_domain, Namespace::Output)],
                    );
                    stream.push(StreamEntry::Map(map));
                }
            },
            Argument::MapArg(map_arg) => {
                let map = Map::new(map_arg.input_key, map_arg.output_keys);
                stream.push(StreamEntry::Map(map));
            },
            Argument::BlockArg(block_arg) => {
                for key in block_arg.keys {
                    stream.push(StreamEntry::Map(Map::block(key)));
                }
            },
            Argument::HookArg(hook_arg) => {
                let mut hook = Hook::new(hook_arg.hold_keys, hook_arg.period);
                for exec_shell in hook_arg.exec_shell {
                    hook.add_command("/bin/sh".to_owned(), vec!["-c".to_owned(), exec_shell]);
                }

                for send_key in hook_arg.send_keys {
                    hook.add_send_key(send_key)
                }
                
                for effect in hook_arg.toggle_action.implement(&state, &toggle_indices)? {
                    hook.add_effect(effect);
                }
                
                stream.push(StreamEntry::Hook(hook));
            },
            Argument::ToggleArg(toggle_arg) => {
                let index = match &toggle_arg.id {
                    Some(id) => toggle_indices.get(id).cloned(),
                    None => None,
                };
                let toggle = Toggle::new(toggle_arg.input_key, toggle_arg.output_keys, toggle_arg.mode, &mut state, index)?;
                stream.push(StreamEntry::Toggle(toggle));
            },
            Argument::PrintArg(print_arg) => {
                stream.push(StreamEntry::Print(print_arg.compile()));
            },
            Argument::MergeArg(merge_arg) => {
                stream.push(StreamEntry::Merge(merge_arg.compile()));
            },
            Argument::DelayArg(delay_arg) => {
                stream.push(StreamEntry::Delay(delay_arg.compile()));
            },
        }
    }

    // Do sanity checks.
    if ! are_unique(output_devices.iter().filter_map(|device| device.create_link.as_ref())) {
        return Err(ArgumentError::new("Multiple output devices cannot create a link at the same location.".to_owned()).into());
    }

    // Compute the capabilities of the output devices.
    let (input_devices, input_capabilities) = crate::io::input::open_and_query_capabilities(input_devices)?;
    let setup = Setup::create(stream, output_devices, state, input_capabilities)?;

    Ok(Implementation { setup, input_devices })
}

/// Returns true if all items in the iterator are unique, otherwise returns false.
fn are_unique<T: Eq>(items: impl Iterator<Item=T>) -> bool {
    let mut seen_items = Vec::new();
    for item in items {
        if seen_items.contains(&item) {
            return false
        }
        seen_items.push(item)
    }
    true
}