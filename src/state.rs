// SPDX-License-Identifier: GPL-2.0-or-later

use std::ops::{Index,IndexMut};
use std::collections::HashMap;
use crate::error::InternalError;
use crate::event::EventCode;
use crate::domain::Domain;

/// Represents the state of the stream that can change as events flow through it.
pub struct State {
    /// Represents the state of --toggle arguments.
    toggles: Vec<ToggleState>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct ToggleIndex(usize);
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct MergeIndex(usize);

impl State {
    pub fn new() -> State {
        State {
            toggles: Vec::new(),
        }
    }

    /// Adds a ToggleState to self and returns the index at which it can be accessed.
    pub fn push_toggle(&mut self, value: ToggleState) -> ToggleIndex {
        self.toggles.push(value);
        ToggleIndex(self.toggles.len() - 1)
    }

    /// Returns all toggles except those with a listed index.
    pub fn get_toggles_except<'a>(&'a mut self, excluded_indices: &'a [ToggleIndex]) -> impl Iterator<Item=&'a mut ToggleState> {
        self.toggles.iter_mut().enumerate().filter(
            move |(index, _)| {
                ! excluded_indices.iter().any(|excluded_index| *index == excluded_index.0)
            }
        ).map(|(_, item)| item)
    }

    pub fn create_toggle_with_size(&mut self, size: usize) -> Result<ToggleIndex, InternalError> {
        let toggle_state = ToggleState::new(size)?;
        Ok(self.push_toggle(toggle_state))
    }
}

impl Index<ToggleIndex> for State {
    type Output = ToggleState;
    fn index(&self, index: ToggleIndex) -> &ToggleState {
        &self.toggles[index.0]
    }
}

impl IndexMut<ToggleIndex> for State {
    fn index_mut(&mut self, index: ToggleIndex) -> &mut ToggleState {
        &mut self.toggles[index.0]
    }
}

pub struct ToggleState {
    /// The current output of this toggle that is active.
    /// Note that this value is zero-indexed, although the user-facing interface is one-indexed.
    value: usize,

    /// The amount of states that can be toggled between.
    size: usize,

    /// If the last value of a specific EventId was not zero, consistent maps will remember
    /// to which index that event was last routed.
    pub memory: HashMap<(EventCode, Domain), usize>,
}

impl ToggleState {
    pub fn new(size: usize) -> Result<ToggleState, InternalError> {
        if size > 0 {
            Ok(ToggleState { size, value: 0, memory: HashMap::new() })
        } else {
            Err(InternalError::new("A toggle requires at least one state."))
        }
    }

    /// Moves this toggle's active output to the next one.
    pub fn advance(&mut self) {
        self.value += 1;
        self.value %= self.size;
    }

    pub fn value(&self) -> usize {
        self.value
    }

    pub fn set_value_wrapped(&mut self, value: usize) {
        self.value = value % self.size
    }

    pub fn size(&self) -> usize {
        self.size
    }
}