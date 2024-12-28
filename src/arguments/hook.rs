// SPDX-License-Identifier: GPL-2.0-or-later

use crate::error::{ArgumentError, InternalError, RuntimeError};
use crate::range::Interval;
use crate::utils;
use crate::state::{State, ToggleIndex};
use crate::stream::hook::{Effect, Trigger, EventDispatcher};
use crate::key::{Key, KeyParser};
use crate::event::{Namespace, EventType};
use crate::arguments::lib::ComplexArgGroup;
use std::collections::HashMap;
use crate::time::Duration;

/// The KeyParser that is used to parse Hook keys.
pub(super) const PARSER: KeyParser = KeyParser {
    allow_transitions: false,
    allow_values: true,
    allow_ranges: true,
    allow_domains: true,
    allow_types: false,
    default_value: "1~",
    allow_relative_values: false,
    type_whitelist: None,
    namespace: Namespace::User,
};

/// Represents a --hook argument.
#[derive(Clone)]
pub(super) struct HookArg {
    /// The keys on which this --hook triggers and their original string representations.
    pub keys_and_str: Vec<(Key, String)>,

    pub exec_shell: Vec<String>,
    pub toggle_action: HookToggleAction,
    pub period: Option<Duration>,
    pub sequential: bool,
    /// Specified by the send-key and send-event clauses.
    pub event_dispatcher: EventDispatcherArg,

    /// Specified by the breaks-on clause. Whenever an event matches one of the following
    /// keys but not one of its keys_and_str, all trackers invalidate.
    pub breaks_on: Vec<Key>,
}

/// I'm undecided on the name of the send-event, so I'm creating a constant for it to make sure I don't forget
/// a reference if I later change it.
const SEND_EVENT_CLAUSE: &str = "send-event";
const SEND_KEY_CLAUSE: &str = "send-key";

impl HookArg {
	pub fn parse(args: Vec<String>) -> Result<HookArg, RuntimeError> {
        let arg_group = ComplexArgGroup::parse(args,
            &["toggle", "sequential"],
            &["exec-shell", "toggle", "period", SEND_KEY_CLAUSE, SEND_EVENT_CLAUSE, "breaks-on"],
            false,
            true,
        )?;

        let toggle_action = HookToggleAction::parse(arg_group.has_flag("toggle"), arg_group.get_clauses("toggle"))?;
        let keys_str = arg_group.keys.clone();
        let keys = PARSER.parse_all(&keys_str)?;
        let keys_and_str = keys.into_iter().zip(keys_str).collect();

        let sequential = arg_group.has_flag("sequential");
        let period = match arg_group.get_unique_clause("period")? {
            None => None,
            Some(value) => Some(crate::arguments::delay::parse_period_value(&value)?),
        };

        // Parse the send-key and send-event clauses.
        let mut event_dispatcher = EventDispatcherArg::new();
        for (name, value) in arg_group.clauses() {
            match name {
                SEND_KEY_CLAUSE => {
                    let key = parse_send_key_clause(value)?;
                    event_dispatcher.add_send_key(key);
                },
                SEND_EVENT_CLAUSE => {
                    let key = parse_send_event_clause(value)?;
                    event_dispatcher.add_send_event(key);
                },
                _ => (),
            }
        };

        let breaks_on = KeyParser::default_filter()
            .parse_all(&arg_group.get_clauses("breaks-on"))?;

        if arg_group.keys.is_empty() {
            Err(ArgumentError::new("A --hook argument requires at least one key.").into())
        } else {
            Ok(HookArg {
                keys_and_str,
                exec_shell: arg_group.get_clauses("exec-shell"),
                toggle_action, period, sequential, event_dispatcher, breaks_on
            })
        }
    }

    pub fn compile_trigger(&self) -> Trigger {
        let keys: Vec<Key> = self.keys_and_str.iter().map(|(key, _)| key.clone()).collect();
        Trigger::new(keys, self.breaks_on.clone(), self.period, self.sequential)
    }
}

#[derive(Clone)]
pub struct EventDispatcherArg {
    /// These events need to be sent when the hook activates in the order specified.
    pub on_press: Vec<Key>,
    /// These events need to be sent when the hook activates *in the order specified*. Events that should be
    /// sent in reverse order such as from send-key will be put into this vector in reverse order.
    pub on_release: Vec<Key>,
}

impl EventDispatcherArg {
    fn new() -> Self {
        EventDispatcherArg {
            on_press: Vec::new(),
            on_release: Vec::new(),
        }
    }

    fn add_send_key(&mut self, key: Key) {
        let mut on_press_key = key.clone();
        on_press_key.set_value(Interval::new(1, 1));
        let mut on_release_key = key;
        on_release_key.set_value(Interval::new(0, 0));

        self.on_press.push(on_press_key);
        self.on_release.insert(0, on_release_key);
    }

    fn add_send_event(&mut self, key: Key) {
        self.on_press.push(key);
    }

    pub fn compile(self) -> EventDispatcher {
        EventDispatcher::new(self.on_press, self.on_release)
    }

    /// Returns an iterator over all events that this hook might send.
    pub fn sendable_events(&self) -> impl Iterator<Item=&Key> {
        let EventDispatcherArg { on_press, on_release } = self;
        on_press.iter().chain(on_release)
    }
}

fn parse_send_key_clause(key: &str) -> Result<Key, RuntimeError> {
    KeyParser {
        allow_transitions: false,
        allow_values: false,
        allow_ranges: false,
        allow_domains: true,
        allow_types: false,
        default_value: "",
        allow_relative_values: false,
        type_whitelist: Some(vec![EventType::KEY]),
        namespace: Namespace::User,
    }.parse(key).map_err(Into::into)
}

fn parse_send_event_clause(key: &str) -> Result<Key, RuntimeError> {
    // You know, I'm starting to think that this whole KeyParser thing needs a change in its interface.
    // After adding so many options to it, it still doesn't have an option to declare "requires event"
    // value, and adding yet another option for that would break some of its other interfaces.
    //
    // As workaround, we just check locally whether this key has an event value.
    let event = KeyParser {
        allow_transitions: false,
        allow_values: true,
        allow_ranges: false,
        allow_domains: true,
        allow_types: false,
        default_value: "",
        allow_relative_values: false,
        type_whitelist: None,
        namespace: Namespace::User,
    }.parse(key)?;

    let (code, value) = event.clone().split_value();
    let code = match code.requires_event_code() {
        Some(code) => code,
        None => return Err(InternalError::new("Parsing failed: no event code was found where one should exist according to an earlier check. This is a bug.").into())
    };
    if value.is_none() {
        return Err(ArgumentError::new(format!(
            "All events sent by the {SEND_EVENT_CLAUSE} clause must have their event value specified, e.g. \"{}:1\"",
            crate::ecodes::event_name(code)
        )).into());
    }

    Ok(event)
}

/// Represents how a single toggle clause on a hook should modify some toggle.
#[derive(Clone, Copy)]
enum HookToggleShift {
    /// Move the active index to the next one, wrapping around.
    Next,
    /// Set the active index to a specific index.
    ToIndex(usize),
}

/// Represents the aggregate effect of all toggle= clauses on a single --hook.
/// This is used to track arguments, this is not the implementation of such an effect.
#[derive(Clone)]
pub struct HookToggleAction {
    /// The action based on a toggle flag or a toggle= without id.
    global_action: Option<HookToggleShift>,
    /// The set of specific toggle=id:index specified.
    by_id_actions: HashMap<String, HookToggleShift>,
}

impl HookToggleAction {
    fn new() -> HookToggleAction {
        HookToggleAction {
            global_action: None,
            by_id_actions: HashMap::new(),
        }
    }

    pub fn parse(has_toggle_flag: bool, toggle_clauses: Vec<String>) -> Result<HookToggleAction, ArgumentError> {
        let mut toggle_action = HookToggleAction::new();
        if has_toggle_flag {
            toggle_action.global_action = Some(HookToggleShift::Next);
        }
        for clause in toggle_clauses {
            let (id, index_str_opt) = utils::split_once(&clause, ":");
            let index: HookToggleShift = match index_str_opt {
                None => HookToggleShift::Next,
                Some(index_str) => HookToggleShift::ToIndex(
                    match index_str.parse::<usize>() {
                        Ok(value) => match value {
                            0 => return Err(ArgumentError::new("Cannot use toggle index 0: toggle indices start at 1.")),
                            _ => value - 1,
                        },
                        Err(error) => return Err(ArgumentError::new(format!("Cannot interpret {} as an integer: {}.", index_str, error))),
                    }
                ),
            };
            match id {
                "" => match toggle_action.global_action {
                    None => { toggle_action.global_action = Some(index); },
                    Some(_) => return Err(ArgumentError::new("A --hook cannot have multiple unspecified toggle clauses.")),
                },
                _ => {
                    match toggle_action.by_id_actions.get(id) {
                        None => { toggle_action.by_id_actions.insert(id.to_owned(), index); },
                        Some(_) => return Err(ArgumentError::new(format!("A toggle={} clause has been specified multiple times.", {id}))),
                    }
                }
            }
        }

        Ok(toggle_action)
    }

    /// Returns a list of all toggle effects that a hook needs to implement this HookToggleAction.
    /// Requires a map mapping toggle's id to their index. This map must contain all toggles which
    /// have an ID, but does not need to contain toggles that don't have any ID.
    pub fn implement(&self, state: &State, toggle_index_by_id: &HashMap<String, ToggleIndex>) -> Result<Vec<Effect>, ArgumentError> {
        let mut effects: Vec<Effect> = Vec::new();
        let mut specified_indices: Vec<ToggleIndex> = Vec::new();
        for (toggle_id, &shift) in &self.by_id_actions {
            let toggle_index = *toggle_index_by_id.get(toggle_id).ok_or_else(|| {
                ArgumentError::new(format!("No toggle with the id \"{}\" exists.", toggle_id))
            })?;

            if let HookToggleShift::ToIndex(target_index) = shift {
                let toggle_size = state[toggle_index].size();
                if target_index >= toggle_size {
                    return Err(ArgumentError::new(format!(
                        "The index {} is out of range for the toggle with id \"{}\".", target_index + 1, toggle_id
                    )))
                }
            }

            specified_indices.push(toggle_index);
            effects.push(Box::new(move |state: &mut State| {
                match shift {
                    HookToggleShift::Next => state[toggle_index].advance(),
                    HookToggleShift::ToIndex(value) => state[toggle_index].set_value_wrapped(value),
                }
            }));
        }
        if let Some(shift) = self.global_action {
            effects.push(Box::new(move |state: &mut State| {
                let toggles_affected = state.get_toggles_except(&specified_indices);
                for toggle in toggles_affected {
                    match shift {
                        HookToggleShift::Next => toggle.advance(),
                        HookToggleShift::ToIndex(value) => toggle.set_value_wrapped(value),
                    }
                }
            }));
        }

        Ok(effects)
    }
}
