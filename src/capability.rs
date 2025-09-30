// SPDX-License-Identifier: GPL-2.0-or-later

use std::collections::{HashMap, HashSet};
use crate::event::{EventType, EventCode, EventValue, Namespace};
use crate::domain::Domain;
use crate::range::{Interval, Set};
use crate::ecodes;
use crate::bindings::libevdev;

const EV_REP_CODES: &[EventCode] = &[
    EventCode::new(EventType::REP, ecodes::REP_DELAY),
    EventCode::new(EventType::REP, ecodes::REP_PERIOD),
];

/// Represents a map that maps an input domain to a list of capabilities which that domain is expected
/// to be able to produce now or in the future.
///
/// The InputCapabilites are often kept track of separately of the InputDevices because the
/// InputCapabilities may also include tentative capabilities of devices that are currently not available.
pub type InputCapabilites = HashMap<Domain, Capabilities>;

/// When we want to know whether a certain capability will trigger a map, we might not
/// be sure because it depends on detailed event or runtime information. In this case,
/// some test may return "Maybe" and we need to hedge our bets against both matching and
/// not matching.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Certainty {
    Always,
    Maybe,
}
use Certainty::{Always, Maybe};

impl PartialOrd for Certainty {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(Ord::cmp(self, other))
    }
}

impl Ord for Certainty {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        let self_val  = match self  {Always => 2, Maybe => 1};
        let other_val = match other {Always => 2, Maybe => 1};
        self_val.cmp(&other_val)
    }
}

#[test]
fn test_certainty_ord() {
    assert!(Certainty::Always > Certainty::Maybe);
    assert!(Certainty::Maybe < Certainty::Always);
    assert!(std::cmp::max(Certainty::Always, Certainty::Maybe) == Certainty::Always);
    assert!(std::cmp::min(Certainty::Always, Certainty::Maybe) == Certainty::Maybe);
}

#[derive(Clone, PartialEq, Eq)]
pub struct Capabilities {
    /// All pairs of (type, code) supported by a device.
    pub codes: HashSet<EventCode>,
    /// Additional information for the EV_ABS event types.
    pub abs_info: HashMap<EventCode, AbsInfo>,
    /// Additional information about the repeat events that happen on EV_KEY, associated with EV_REP.
    pub rep_info: Option<RepeatInfo>,
}

/// Represents the value related to EV_REP.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct RepeatInfo {
    /// REP_DELAY
    pub delay: EventValue,
    /// REP_PERIOD
    pub period: EventValue,
}

impl RepeatInfo {
    /// The kernel is ultimately going to ignore our choice of repeat info anyway, but we like
    /// to keep track of the real values in case the kernel gets fixed sometime. In case we don't
    /// have access to real values, this tells us what the kernel defaults are.
    pub fn kernel_default() -> RepeatInfo {
        RepeatInfo {
            delay: 250,
            period: 33,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct AbsInfo {
    pub min_value: EventValue,
    pub max_value: EventValue,
    pub meta: AbsMeta,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct AbsMeta {
    pub fuzz: i32,
    pub flat: i32,
    pub resolution: i32,
    pub value: i32,
}

impl AbsMeta {
    pub fn default() -> AbsMeta {
        AbsMeta {
            flat: 0, fuzz: 0, resolution: 0, value: 0
        }
    }
}

impl AbsInfo {
    /// Tells you whether this AbsInfo is equal to the other AbsInfo up to the current value.
    /// You know, maybe it was a bad idea to include the value in the capabilities. But we do need to give a current
    /// value to libevdev, and the one that originates from some input device is the one most likely to be correct...
    /// TODO (Medium Priority): Consider using a better method to discover the initial state of the absolute axes
    ///                         on the output devices.
    pub fn is_equivalent_to(self, other: AbsInfo) -> bool {
        let AbsInfo { min_value, max_value, meta } = self;
        let AbsMeta { fuzz, flat, resolution, value: _ } = meta;

        min_value == other.min_value &&
            max_value == other.max_value &&
            fuzz == other.meta.fuzz &&
            flat == other.meta.flat &&
            resolution == other.meta.resolution
    }
}

impl From<AbsInfo> for libevdev::input_absinfo {
    fn from(abs_info: AbsInfo) -> libevdev::input_absinfo {
        libevdev::input_absinfo {
            flat: abs_info.meta.flat,
            fuzz: abs_info.meta.fuzz,
            resolution: abs_info.meta.resolution,
            minimum: std::cmp::min(abs_info.min_value, abs_info.max_value),
            maximum: std::cmp::max(abs_info.min_value, abs_info.max_value),
            value: abs_info.meta.value,
        }
    }
}

impl From<libevdev::input_absinfo> for AbsInfo {
    fn from(abs_info: libevdev::input_absinfo) -> AbsInfo {
        AbsInfo {
            min_value: abs_info.minimum,
            max_value: abs_info.maximum,
            meta: AbsMeta {
                flat: abs_info.flat,
                fuzz: abs_info.fuzz,
                resolution: abs_info.resolution,
                value: abs_info.value,
            }
        }
    }
}

impl Capabilities {
    pub fn new() -> Capabilities {
        Capabilities {
            codes: HashSet::new(),
            abs_info: HashMap::new(),
            rep_info: None,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.codes.is_empty()
    }

    /// Returns true if this Capabilities is not capable of any non-trivial event codes, where
    /// events such as EV_SYN or EV_REP are deemed trivial.
    pub fn has_no_content(&self) -> bool {
        let trivial_types = [EventType::SYN, EventType::REP];
        return self.codes.iter().all(|code| trivial_types.contains(&code.ev_type()));
    }

    pub fn to_vec_from_domain_and_namespace(&self, domain: Domain, namespace: Namespace) -> Vec<Capability> {
        self.codes.iter().filter_map(|&code| {
            let abs_info = self.abs_info.get(&code);
            let (value_range, abs_meta) = match abs_info {
                None => match code.ev_type() {
                    EventType::KEY => (Interval::new(Some(0), Some(2)), None),
                    _ => (Interval::new(None, None), None),
                },
                Some(info) => (
                    Interval::new(Some(info.min_value), Some(info.max_value)),
                    Some(info.meta),
                ),
            };

            // The EV_REP capability signifies that the kernel has been asked to automatically
            // generate repeat events, not the ability to generate repeat events. As such, it
            // doesn't make sense to propagate EV_REP capabilities.
            if code.ev_type().is_rep() {
                None
            } else {
                Some(Capability { code, domain, values: Set::from(value_range), abs_meta, namespace })
            }
        }).collect()
    }

    pub fn add_capability(&mut self, cap: Capability) {
        if cap.values.is_empty() {
            return;
        }

        if cap.code.ev_type().is_abs() {
            // It is possible to lack abs_meta on this capability, e.g. if some non-abs event got
            // mapped to an abs-event. In that case, use the sanest default we can think of.
            let meta = match cap.abs_meta {
                Some(meta) => meta,
                None => AbsMeta::default(),
            };
            let range = cap.values.spanning_interval().expect("Internal error: a capability can assume a nonempty set of values, yet its spanning range is empty. This is a bug.");
            let abs_info = AbsInfo {
                min_value: range.min,
                max_value: range.max,
                meta,
            };
            self.add_abs(cap.code, abs_info);

        } else {
            self.add_non_abs(cap.code);
        }
    }

    pub fn add_non_abs(&mut self, code: EventCode) {
        assert!(!code.ev_type().is_abs());
        self.codes.insert(code);
    }

    /// Adds an EV_ABS capability. Panics if the code is not the EV_ABS type.
    pub fn add_abs(&mut self, code: EventCode, abs_info: AbsInfo) {
        assert!(code.ev_type().is_abs());
        self.codes.insert(code);

        // Check if we already know something about this axis from another source. If so, we
        // should merge this capability with that one. Otherwise, for code simplicity we assume
        // that the current info is the same as that of this new capability.
        let existing_info = self.abs_info.get(&code);
        let cap_range = Interval { min: abs_info.min_value, max: abs_info.max_value };
        let meta = abs_info.meta;
        let (current_range, current_meta) = match existing_info {
            Some(info) => (Interval::new(Some(info.min_value), Some(info.max_value)), info.meta),
            None => (cap_range, abs_info.meta),
        };

        // Merge the current info with this capability.
        let new_range = current_range.merge(&cap_range);
        let new_meta = AbsMeta {
            // Merging is hard. I don't know whether min or max is most appropriate for these.
            flat: std::cmp::min(current_meta.flat, meta.flat),
            fuzz: std::cmp::min(current_meta.fuzz, meta.fuzz),
            resolution: std::cmp::max(current_meta.resolution, meta.resolution),
            value: new_range.bound(meta.value),
        };

        let min_value = new_range.min;
        let max_value = new_range.max;

        // Insert or overwrite the existing value.
        self.abs_info.insert(code, AbsInfo {
            min_value, max_value, meta: new_meta
        });
    }

    /// Adds EV_REP capabilities to self with arbitrary delay and period.
    /// The kernel is going to ignore the delay and period we give it anyway.
    pub fn require_ev_rep(&mut self) {
        if self.rep_info.is_none() {
            self.set_ev_rep(RepeatInfo::kernel_default())
        }
    }

    /// Removes EV_REP cababilities from self.
    pub fn remove_ev_rep(&mut self) {
        self.rep_info = None;
        for code in EV_REP_CODES {
            self.codes.remove(code);
        }
    }

    /// Sets the rep_info variable of self and makes sure that the correct capabilities
    /// are inserted to self.codes.
    fn set_ev_rep(&mut self, repeat_info: RepeatInfo) {
        self.rep_info = Some(repeat_info);
        for &code in EV_REP_CODES {
            self.codes.insert(code);
        }
    }

    pub fn ev_types(&self) -> HashSet<EventType> {
        self.codes.iter()
            .map(|code| code.ev_type())
            .collect()
    }

    /// Returns the abs info as a vector of (EventCode, AbsInfo) pairs that has been sorted by event code.
    pub fn abs_info_to_sorted_vec(&self) -> Vec<(EventCode, AbsInfo)> {
        let mut result: Vec<(EventCode, AbsInfo)> = self.abs_info.iter().map(|(&k, &v)| (k, v)).collect();
        result.sort_by_key(|&(code, _)| code);
        result
    }

    /// Given a device that has output capabilities `other`, can we properly write all events corrosponding
    /// to the capabilities of `self` to that device? Returns true if we can, false if there may be issues.
    ///
    /// To be true, `other` must have all event codes of `self` and identical absolute axes. Ignores the
    /// current value of absolute axes.
    pub fn is_compatible_with(&self, other: &Capabilities) -> bool {
        if ! self.codes.is_subset(&other.codes) {
            return false;
        }
        for (code, info) in &self.abs_info {
            if let Some(other_info) = other.abs_info.get(code) {
                // Avoid getting incompatibility due to a different meta.value, but do compare all
                // other properties of the absolute axes.
                let mut other_info: AbsInfo = *other_info;
                other_info.meta.value = info.meta.value;

                if *info != other_info {
                    return false;
                }
            } else {
                return false;
            }
        }
        // We don't care about self.rep_info because the kernel doesn't either.

        true
    }

    /// Tells you whether these capabilities are equal to the other capabilities up to the current state of
    /// the absolute axes.
    pub fn is_equivalent_to(&self, other: &Capabilities) -> bool {
        // This destructure happens to intentionally cause a compilation error if we add additional fields.
        let Capabilities { codes, abs_info: _, rep_info } = self;
        if !(codes == &other.codes && rep_info == &other.rep_info) {
            return false;
        }

        let self_abs_as_vec = self.abs_info_to_sorted_vec();
        let other_abs_as_vec = other.abs_info_to_sorted_vec();
        if self_abs_as_vec.len() != other_abs_as_vec.len() {
            return false;
        }

        self_abs_as_vec.into_iter().zip(other_abs_as_vec).all(
            |((self_code, self_abs), (other_code, other_abs))| {
                self_code == other_code && self_abs.is_equivalent_to(other_abs)
            }
        )
    }
}

#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct Capability {
    pub code: EventCode,
    pub domain: Domain,
    pub namespace: Namespace,
    pub values: Set,
    pub abs_meta: Option<AbsMeta>,
}

impl Capability {
    /// Returns a copy of self with an universal value range, and the original range.
    pub fn split_values(mut self) -> (Capability, Set) {
        let value = self.values;
        self.values = Set::empty();
        (self, value)
    }

    /// Returns a copy of self with the given value range.
    pub fn with_values(&self, values: Set) -> Capability {
        Capability {
            code: self.code,
            domain: self.domain,
            namespace: self.namespace,
            values,
            abs_meta: self.abs_meta,
        }
    }

    /// Returns the same capability with a different set of values.
    pub fn map_values(&self, map: impl Fn(&Set) -> Set) -> Capability {
        Capability {
            code: self.code,
            domain: self.domain,
            namespace: self.namespace,
            values: map(&self.values),
            abs_meta: self.abs_meta,
        }
    }
}

/// Tries to simplify a vec of capabilites by merging similar capabilities (those that differ
/// only in value) together. Also dumps all empty capabilities.
pub fn aggregate_capabilities(capabilities: Vec<Capability>) -> Vec<Capability> {
    // Sort the capabilities into those which only differ by value. If two capabilities differ
    // only by their values, perform a setunion on their values.
    let mut values_by_capability: HashMap<Capability, Set> = HashMap::new();
    for capability in capabilities {
        let (key, values) = capability.split_values();
        let entry = values_by_capability.entry(key);
        match entry {
            std::collections::hash_map::Entry::Vacant(vacant_entry) => {
                vacant_entry.insert(values);
            },
            std::collections::hash_map::Entry::Occupied(occupied_entry) => {
                let recorded_values = occupied_entry.into_mut();
                *recorded_values = recorded_values.union(&values);
            },
        }
    }

    // Turn the HashMap back into a vector.
    let mut result: Vec<Capability> = Vec::new();
    for (key, values) in values_by_capability {
        result.push(key.with_values(values));
    }

    result
}

/// Given an InputCapabilites, generates a vector that contains every discrete capability that can be
/// generated by the corresponding input devices.
pub fn input_caps_to_vec(caps: &InputCapabilites) -> Vec<Capability> {
    caps.iter()
        .flat_map(|(domain, caps)| caps.to_vec_from_domain_and_namespace(*domain, Namespace::Input))
        .collect()
}