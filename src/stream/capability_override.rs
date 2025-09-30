use std::collections::HashMap;
use crate::arguments::capability::CapabilityOverrideSpec;
use crate::capability::Capability;
use crate::domain::Domain;
use crate::event::{EventCode, Namespace};
use crate::range::{Interval, Set};

pub struct CapabilityOverride {
    device: Domain,

    /// The output device with the given domain must have at least the following capabilities in addition to whatever
    /// is automatically inferred. Furthermore, if the range specified in the following capabilities disagrees with
    /// the inferred range, then the following range has priority.
    forced_capabilities: HashMap<EventCode, CapabilityOverrideSpec>
}

impl CapabilityOverride {
    pub fn new(device: Domain, forced_capabilities: HashMap<EventCode, CapabilityOverrideSpec>) -> CapabilityOverride {
        CapabilityOverride {
            device,
            forced_capabilities,
        }
    }

    // CapabilityOverride does not alter the events themselves, so there is no `apply()` or `apply_to_all()` here.
    pub fn apply_to_all_caps(&self, caps: &[Capability], caps_out: &mut Vec<Capability>) {
        let mut capabilities_not_yet_forced = self.forced_capabilities.clone();

        // Pass on all capabilities. If an overridden range has been specified for a capability,
        // use the specified range instead of the inferred range.
        for cap in caps {
            // TODO (HIGH-PRIORITY, CRITICAL BUG)
            // This does NOT work because the capability override is put in the stream BEFORE the output device,
            // so Namespace::Output will not be encountered!
            if cap.domain != self.device || cap.namespace != Namespace::Output {
                caps_out.push(cap.clone());
                continue;
            }

            // Important: because a capability with she same code can show up multiple times, it is important to
            // compare against `self.forced_capabilities` instead of `capabilities_not_yet_forced` here.
            if let Some(override_spec) = self.forced_capabilities.get(&cap.code) {
                // TODO (high-priority) Also override the other parts of the capability
                if let Some(range) = override_spec.range {
                    caps_out.push(cap.with_values(Set::from_unordered_intervals(vec![range])));
                } else {
                    caps_out.push(cap.clone());
                }
            } else {
                caps_out.push(cap.clone());
            }
            capabilities_not_yet_forced.remove(&cap.code);
        }

        // All capabilities that are forced but were not taken care of in the above loop shall be handled now.
        for (code, spec) in capabilities_not_yet_forced {
            let values = Set::from_unordered_intervals(vec![
                spec.range.unwrap_or_else(|| Interval::new(None, None))
            ]);
            caps_out.push(Capability {
                code, values,
                domain: self.device,
                namespace: Namespace::Output,
                abs_meta: None, // TODO: high priority
            });
        }
    }
}
