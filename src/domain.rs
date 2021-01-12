// SPDX-License-Identifier: GPL-2.0-or-later

use std::collections::HashMap;
use std::sync::Mutex;
use crate::error::ArgumentError;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Domain(usize);

pub fn get_unique_domain() -> Domain {
    TRACKER.lock()
        .expect("Fatal error: internal mutex poisoned.")
        .get_unique_domain()
}

/// Returns a Domain for the given name. Always returns the same string for the same name
/// in the context of a single program execution. If the provided name is not up to naming
/// standards, returns an error.
///
/// The internal domain name used must NOT start with an @.
pub fn resolve(name: &str) -> Result<Domain, ArgumentError> {
    TRACKER.lock()
        .expect("Fatal error: internal mutex poisoned.")
        .resolve(name)
}

/// Returns a String that resolves to this Domain, if it exists. Otherwise, returns None.
pub fn try_reverse_resolve(domain: Domain) -> Option<String> {
    TRACKER.lock()
        .expect("Fatal error: internal mutex poisoned.")
        .try_reverse_resolve(domain)
}

lazy_static!{
    static ref TRACKER: Mutex<DomainTracker> = Mutex::new(DomainTracker::new());
}

/// The DomainTracker is a responsible for converting strings to domains (e.g. turns the
/// "foo" part of "key:a@foo" into a usize), and also for handing out unique domains for some
/// purposes where events need a domain that is not accessible to the user.
/// 
/// Intended to be used as a singleton.
struct DomainTracker {
    name_map: HashMap<String, Domain>,
    reveres_name_map: HashMap<Domain, String>,
    /// A counter for how many domains have been allocated. Used to allocate new unique domains.
    counter: usize,
}

impl DomainTracker {
    /// Maps a string to some domain. Always returns the same domain for the same string.
    fn resolve(&mut self, name: &str) -> Result<Domain, ArgumentError> {
        if name.starts_with('@') {
            return Err(ArgumentError::new(format!("The domain \"{}\" may not start with an @.", name)));
        }
        if name == "" {
            return Err(ArgumentError::new("Domains may not be empty."));
        }

        Ok(match self.name_map.get(name) {
            Some(&domain) => domain,
            None => {
                let new_domain = self.get_unique_domain();
                self.name_map.insert(name.to_owned(), new_domain);
                self.reveres_name_map.insert(new_domain, name.to_owned());
                new_domain
            }
        })
    }

    fn try_reverse_resolve(&mut self, domain: Domain) -> Option<String> {
        self.reveres_name_map.get(&domain).cloned()
    }

    /// Returns an arbitrary domain. Said domain will never be returned again by this namespace.
    fn get_unique_domain(&mut self) -> Domain {
        let result = self.counter;
        self.counter += 1;
        Domain(result)
    }

    fn new() -> DomainTracker {
        DomainTracker {
            name_map: HashMap::new(),
            reveres_name_map: HashMap::new(),
            counter: 0,
        }
    }
}
