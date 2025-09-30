
use std::fmt::Write;
use std::collections::HashMap;

use crate::domain::Domain;
use crate::error::{ArgumentError, Context};
use crate::event::EventCode;
use crate::key::KeyParser;
use crate::range::Interval;
use crate::stream::capability_override::CapabilityOverride;

use super::lib::ComplexArgGroup;

/// Represents a --capability argument.
pub(super) struct CapabilityArg {
    pub overrides: CapabilityOverrideSet,
}

#[derive(Clone, Copy)]
pub struct CapabilityOverrideSpec {
    pub range: Option<Interval>,
    pub flat: Option<i32>,
    pub fuzz: Option<i32>,
    pub value: Option<i32>,
}

pub struct CapabilityOverrideSet {
    pub inner: HashMap<EventCode, CapabilityOverrideSpec>,
}

/// Returned when incompatible capability overrides are specified.
pub struct IncompatibleError;

impl CapabilityOverrideSet {
    pub fn new() -> Self {
        Self { inner: HashMap::new() }
    }

    /// Adds a new specification to this set, unless a confliction specification already exists.
    pub fn try_add(&mut self, code: EventCode, additional_spec: CapabilityOverrideSpec) -> Result<(), IncompatibleError> {
        use std::collections::hash_map::Entry;
        match self.inner.entry(code) {
            Entry::Occupied(mut occupied_entry) => {
                let current_spec: CapabilityOverrideSpec = *occupied_entry.get();
                *occupied_entry.get_mut() = current_spec.try_merge_with(additional_spec)?;
                Ok(())
            },
            Entry::Vacant(vacant_entry) => {
                vacant_entry.insert(additional_spec);
                Ok(())
            },
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = (&EventCode, &CapabilityOverrideSpec)> {
        self.inner.iter()
    }
}

impl CapabilityOverrideSpec {
    /// Returns an override that specifies the requirements from both specifications. Returns an error if the specifications
    /// are incompatible (e.g. both require a different range.)
    pub fn try_merge_with(self, other: CapabilityOverrideSpec) -> Result<CapabilityOverrideSpec, IncompatibleError> {
        fn merge_options<T: Eq>(opt_1: Option<T>, opt_2: Option<T>) -> Result<Option<T>, IncompatibleError> {
            match (opt_1, opt_2) {
                (None, None) => Ok(None),
                (None, Some(val)) | (Some(val), None) => Ok(Some(val)),
                (Some(val_1), Some(val_2)) => if val_1 == val_2 { Ok(Some(val_1)) } else { Err(IncompatibleError) },
            }
        }

        Ok(CapabilityOverrideSpec {
            range: merge_options(self.range, other.range)?,
            flat:  merge_options(self.flat, other.flat)?,
            fuzz:  merge_options(self.fuzz, other.fuzz)?,
            value: merge_options(self.value, other.value)?,
        })
    }
}


impl CapabilityArg {
	pub fn parse(args: Vec<String>) -> Result<CapabilityArg, ArgumentError> {
        const FLAT: &'static str = "flat";
        const FUZZ: &'static str = "fuzz";
        const VALUE: &'static str = "value";

        let arg_group = ComplexArgGroup::parse(args,
            &[],
            &[FLAT, FUZZ, VALUE],
            false,
            true,
        )?;

        let parser = KeyParser {
            default_value: "",
            allow_values: true,
            allow_ranges: true,
            // Domains are not actually allowed, but the parser accepts domains so this function can give a more
            // helpful error message.
            allow_domains: true,
            allow_transitions: false,
            allow_types: false,
            allow_relative_values: false,
            type_whitelist: None,
            namespace: crate::event::Namespace::Output,
        };

        if arg_group.keys.is_empty() {
            return Err(ArgumentError::new("A --with-capability argument requires at least one key to be specified, e.g. \"--with-capability key:a\" or \"--with-capability abs:x:-127~128\"."))
        }

        let flat: Option<i32> = arg_group.get_unique_clause_i32(FLAT)?;
        let fuzz: Option<i32> = arg_group.get_unique_clause_i32(FUZZ)?;
        let initial_value: Option<i32> = arg_group.get_unique_clause_i32(VALUE)?;

        let mut overrides = CapabilityOverrideSet::new();
        for key_str in &arg_group.keys {
            let key_parse_context_msg = || format!("While parsing the key \"{}\":", key_str);
            let key = parser.parse(&key_str)?;
            if key.requires_domain().is_some() {
                return Err(ArgumentError::new(format!(
                    "The --with-capability argument modifies the capabilities of an output device from the Linux kernel's perspective. Because domains are an evsieve-specific concept that do not exist in Linux, it makes no sense to specify a domain here."
                )).with_context_of(key_parse_context_msg));
            }

            let (code_key, range_opt) = key.split_value();
            let code = match code_key.requires_event_code() {
                Some(code) => code,
                None => return Err(ArgumentError::new(format!(
                    "Each capability key must specify a single event code, e.g. \"key:a\" or \"abs:x:-127~128\" instead of \"key\" or \"abs\".")
                ).with_context_of(key_parse_context_msg)),
            };

            if code.ev_type().is_abs() {
                if range_opt.is_none() {
                    let type_name = crate::ecodes::type_name(code.ev_type());
                    return Err(ArgumentError::new(format!(
                        "When enabling abs-type events, you must specify the range of values that these events can take. Suppose you want a range from -127 to +128, there are two ways you could specify it:\n\t(1) --with-capability {}:-127~128\n\t(2) --with-capability {} range=-127~128",
                        type_name, type_name
                    )).with_context_of(key_parse_context_msg))
                }
            } else {
                if range_opt.is_some() {
                    return Err(ArgumentError::new(format!(
                        "No value ranges can be specified for {}-type events. Value ranges can only be specified for abs-type events.",
                        crate::ecodes::type_name(code.ev_type())
                    )).with_context_of(key_parse_context_msg))
                }
            }

            if overrides.try_add(code, CapabilityOverrideSpec {
                range: range_opt,
                flat, fuzz, value: initial_value,
            }).is_err() {
                return Err(ArgumentError::new(format!("The capability override {} is incompatible with a previously specified capability.", key_str)));
            };
        }

        // Sanity check: only allow the clauses flat, fuzz and value if at least one of the specified capabilities
        // is an EV_ABS capability.
        if ! overrides.inner.keys().any(|key| key.ev_type().is_abs()) {
            let improper_clauses = IntoIterator::into_iter([FLAT, FUZZ, VALUE])
                    .filter(|clause| !arg_group.get_clauses(clause).is_empty())
                    .collect::<Vec<_>>();

            if !improper_clauses.is_empty() {
                let mut error_msg = "The".to_owned();
                if let &[single_clause] = improper_clauses.as_slice() {
                    write!(error_msg, " clause {} is", single_clause).unwrap();
                } else {
                    write!(error_msg, " clauses {} are", improper_clauses.join(", ")).unwrap();
                }
                write!(error_msg, " only applicable for EV_ABS-type events (e.g. abs:x).").unwrap();
                return Err(ArgumentError::new(error_msg));
            }
        }

        Ok(CapabilityArg { overrides })

    }

    pub fn compile(self, device: Domain) -> CapabilityOverride {
        CapabilityOverride::new(device, self.overrides.inner)
    }
}
