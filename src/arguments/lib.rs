// SPDX-License-Identifier: GPL-2.0-or-later

use crate::utils::split_once;
use crate::error::ArgumentError;

/// A ComplexArgGroup represents a group like "--input /dev/keyboard domain=foo grab",
/// containing paths like "/dev/keyboard", flags like "grab" and clauses like "domain=foo".
///
/// Depending on the type of argument, some clauses may be specified multiple times,
/// other times there may be at most one of them.
pub(super) struct ComplexArgGroup {
    /// In the example, this would be "--input"
    pub name: String,                       
    /// In the example, this would be ["grab"]
    flags: Vec<String>,
    /// In the example, this would be [("domain", "foo")]
    clauses: Vec<(String, String)>,

    /// Any stray keys that show up in the argument list.
    pub keys: Vec<String>,
    pub paths: Vec<String>,
}

impl ComplexArgGroup {
    pub fn parse(args: Vec<String>,
            supported_flags: &[&str],
            supported_clauses: &[&str],
            supports_paths: bool,
            supports_keys: bool) -> Result<ComplexArgGroup, ArgumentError> {
        
        let mut args_iter = args.into_iter();
        let arg_name = args_iter.next().ok_or_else(|| ArgumentError::new(
            "Internal error: created an argument group out of no arguments."
        ))?;

        let mut flags: Vec<String> = Vec::new();
        let mut clauses: Vec<(String, String)> = Vec::new();
        let mut keys: Vec<String> = Vec::new();
        let mut paths: Vec<String> = Vec::new();
    
        for arg in args_iter {
            // Check whether this argument is a key.
            if crate::key::resembles_key(&arg) {
                if supports_keys {
                    keys.push(arg);
                    continue;
                } else {
                    return Err(ArgumentError::new(format!(
                        "The {} argument doesn't take any keys: \"{}\"", arg_name, arg
                    )))
                }
            }

            // Check whether this argument is a path.
            if is_path(&arg) {
                if supports_paths {
                    paths.push(arg);
                    continue;
                } else {
                    return Err(ArgumentError::new(format!(
                        "The {} argument doesn't take any paths: \"{}\"", arg_name, arg
                    )))
                }
            }

            // Check whether this argument is a flag or clause.
            let (name, value_opt) = split_once(&arg, "=");
            let name = name.to_string();
            // Check if it's a clause.
            if let Some(value) = value_opt {
                if supported_clauses.contains(&name.as_str()) {
                    clauses.push((name.to_string(), value.to_string()));
                } else {
                    return Err(ArgumentError::new(
                        match supported_flags.contains(&name.as_str()) {
                            true => format!("The {} argument's {} flag doesn't accept a value. Try removing the  \"={}\" part.", arg_name, name, value),
                            false => format!("The {} argument doesn't accept a {} clause: \"{}\"", arg_name, name, arg),
                        }
                    ));
                }
            // Check is it's a flag.
            } else if supported_flags.contains(&name.as_str()) {
                if ! flags.contains(&name) {
                    flags.push(name);
                } else {
                    return Err(ArgumentError::new(format!(
                        "The {} flag has been provided multiple times.", name
                    )))
                }
            // Otherwise, return an error.
            } else {
                return Err(ArgumentError::new(
                    match supported_clauses.contains(&name.as_str()) {
                        true => format!("The {} argument's {} clause requires some value: \"{}=something\".", arg_name, name, name),
                        false => format!("The {} argument doesn't take a {} flag.", arg_name, name),
                    }
                ))
            }
        }

        Ok(ComplexArgGroup {
            name: arg_name, flags, clauses, keys, paths,
        })
    }

    pub fn has_flag(&self, name: &str) -> bool {
        self.flags.contains(&name.to_owned())
    }

    pub fn get_clauses(&self, name: &str) -> Vec<String> {
        self.clauses.iter().cloned().filter_map(|(clause_name, value)| {
            if name == clause_name {
                Some(value)
            } else {
                None
            }
        }).collect()
    }

    /// Get a clause of which at most one may exist.
    /// Multiple copies of this clause will return an error, zero copies will return None.
    pub fn get_unique_clause(&self, name: &str) -> Result<Option<String>, ArgumentError> {
        let clauses = self.get_clauses(name);
        match clauses.len() {
            1 => Ok(Some(clauses[0].clone())),
            0 => Ok(None),
            _ => Err(ArgumentError::new(format!(
                "Multiple copies of the {}= clause have been provided to {}.", name, self.name
            ))),
        }
    }

    pub fn require_paths(&self) -> Result<Vec<String>, ArgumentError> {
        match self.paths.len() {
            0 => Err(ArgumentError::new(format!(
                "The {} argument requires a path. Remember that all paths must be provided as absolute paths.", self.name,
            ))),
            _ => Ok(self.paths.clone()),
        }
    }

    pub fn require_keys(&self) -> Result<Vec<String>, ArgumentError> {
        match self.keys.len() {
            0 => Err(ArgumentError::new(format!(
                "The {} argument requires a key.", self.name,
            ))),
            _ => Ok(self.keys.clone()),
        }
    }

    /// Returns all keys this ComplexArgGroup has. If it has no keys, it will return
    /// a single "" key instead.
    pub fn get_keys_or_empty_key(&self) -> Vec<String> {
        match self.keys.len() {
            0 => vec!["".to_string()],
            _ => self.keys.clone(),
        }
    }

    /// Returns the value of the given clause. If no such clause is specified but a flag with
    /// the same name is specified, returns the value of `default_if_flag`.
    pub fn get_unique_clause_or_default_if_flag(&self, clause_or_flag: &str, default_if_flag: &str) -> Result<Option<String>, ArgumentError> {
        if self.has_flag(clause_or_flag) && ! self.get_clauses(clause_or_flag).is_empty() {
            return Err(ArgumentError::new(format!(
                "Cannot specify both the {} flag an a {} clause.", clause_or_flag, clause_or_flag
            )));
        }
        Ok(match self.get_unique_clause(clause_or_flag)? {
            Some(value) => Some(value),
            None => match self.has_flag(clause_or_flag) {
                true => Some(default_if_flag.into()),
                false => None,
            },
        })
    }
}

pub(super) fn is_path(path: &str) -> bool {
    path.starts_with('/')
}