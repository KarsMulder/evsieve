// SPDX-License-Identifier: GPL-2.0-or-later

use std::cmp::Ordering;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum ExtendedInteger {
    PositiveInfinity,
    NegativeInfinity,
    Discrete(i32),
}

impl ExtendedInteger {
    /// Returns its own value if discrete, otherwise returns a given default value.
    pub fn discrete_or(self, value: i32) -> i32 {
        match self {
            ExtendedInteger::Discrete(discrete_value) => discrete_value,
            _ => value,
        }
    }

    /// Returns true is self and other differ by exactly one.
    pub fn is_acjadent(self, other: ExtendedInteger) -> bool {
        match (self, other) {
            (ExtendedInteger::Discrete(value), ExtendedInteger::Discrete(other_value)) => {
                match value.checked_sub(other_value) {
                    Some(difference) => difference.abs() == 1,
                    None => false,
                }
            },
            _ => false,
        }
    }
}

impl From<i32> for ExtendedInteger {
    fn from(value: i32) -> ExtendedInteger {
        ExtendedInteger::Discrete(value)
    }
}

impl PartialOrd for ExtendedInteger {
    fn partial_cmp(&self, other: &ExtendedInteger) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for ExtendedInteger {
    fn cmp(&self, other: &ExtendedInteger) -> Ordering {
        match self {
            ExtendedInteger::PositiveInfinity => match other {
                ExtendedInteger::PositiveInfinity => Ordering::Equal,
                _ => Ordering::Greater,
            },
            ExtendedInteger::NegativeInfinity => match other {
                ExtendedInteger::NegativeInfinity => Ordering::Equal,
                _ => Ordering::Less,
            },
            ExtendedInteger::Discrete(value) => match other {
                ExtendedInteger::PositiveInfinity => Ordering::Less,
                ExtendedInteger::NegativeInfinity => Ordering::Greater,
                ExtendedInteger::Discrete(other_value) => value.cmp(other_value)
            }
        }
    }
}

/// A bound for the values of an Event's current value or previous value.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub struct Range {
    pub min: ExtendedInteger,
    pub max: ExtendedInteger,
}

impl Range {
    pub fn new(min: Option<i32>, max: Option<i32>) -> Range {
        Range {
            min: match min {
                Some(value) => ExtendedInteger::Discrete(value),
                None => ExtendedInteger::NegativeInfinity,
            },
            max: match max {
                Some(value) => ExtendedInteger::Discrete(value),
                None => ExtendedInteger::PositiveInfinity,
            },
        }
    }

    /// Checks whether this Range contains a value.
    pub fn contains(&self, value: i32) -> bool {
        let extended_value: ExtendedInteger = value.into();
        self.min <= extended_value && self.max >= extended_value
    }

    /// Returns the closest integer to value that lies within this Range.
    pub fn bound(&self, value: i32) -> i32 {
        if let ExtendedInteger::Discrete(min_value) = self.min {
            if value < min_value {
                return min_value;
            }
        }
        if let ExtendedInteger::Discrete(max_value) = self.max {
            if value > max_value {
                return max_value;
            }
        }
        value
    }

    /// Returns the range that would be generated if we bounded every value in the other range.
    pub fn bound_range(&self, other: &Range) -> Range {
        // If we overlap, every bounded value will lie in that overlapping.
        if let Some(intersection) = self.intersect(other) {
            intersection
        // Otherwise all values will be projected to a single point, depending on whether the
        // other range lies entirely above or below this range.
        } else if self.min > other.max {
            Range { min: self.min, max: self.min }
        } else {
            Range { min: self.max, max: self.max }
        }
    }

    /// Returns the largest range that is contained by both self and other.
    pub fn intersect(&self, other: &Range) -> Option<Range> {
        let max = std::cmp::min(self.max, other.max);
        let min = std::cmp::max(self.min, other.min);
        if min > max {
            None
        } else {
            Some(Range {min, max})
        }
    }

    /// Returns the smallest range that contains both self and other.
    /// We don't call this `union` because values that are in neither original range
    /// may show up in the merged range.
    pub fn merge(&self, other: &Range) -> Range {
        let min = std::cmp::min(self.min, other.min);
        let max = std::cmp::max(self.max, other.max);

        Range { min, max }
    }

    /// Returns a range if there is a contiguous range that is the union of both of these.
    /// If such a range does not exist (e.g. there is empty space between them), returns None.
    pub fn try_union(&self, other: &Range) -> Option<Range> {
        if self.intersect(other) == None &&
           ! self.max.is_acjadent(other.min) &&
           ! self.min.is_acjadent(other.max)
        {
            return None;
        }

        Some(Range {
            min: std::cmp::min(self.min, other.min),
            max: std::cmp::max(self.max, other.max),
        })
    }

    /// Tests whether this range is a subset of another range.
    pub fn is_subset_of(&self, other: &Range) -> bool {
        self.intersect(other) == Some(*self)
    }

    /// Tests whether these ranges have no overlap.
    pub fn is_disjoint_with(&self, other: &Range) -> bool {
        self.intersect(other) == None
    }

    /// Returns whether this range is bounded in a mathematical sense.
    pub fn is_bounded(&self) -> bool {
        self.min > ExtendedInteger::NegativeInfinity && self.max < ExtendedInteger::PositiveInfinity
    }
}

#[test]
fn unittest() {
    // Intersection test
    assert_eq!(
        Range::new(Some(1), Some(3)).intersect(&Range::new(Some(2), Some(4))),
        Some(Range::new(Some(2), Some(3)))
    );
    assert_eq!(
        Range::new(Some(2), Some(4)).intersect(&Range::new(Some(1), Some(3))),
        Some(Range::new(Some(2), Some(3)))
    );
    assert_eq!(
        Range::new(Some(1), Some(3)).intersect(&Range::new(Some(5), Some(7))),
        None,
    );
    assert_eq!(
        Range::new(Some(1), Some(3)).intersect(&Range::new(Some(-4), Some(-2))),
        None,
    );
    assert_eq!(
        Range::new(Some(1), Some(3)).intersect(&Range::new(Some(-4), None)),
        Some(Range::new(Some(1), Some(3))),
    );

    //Bounding tests.
    assert_eq!(
        Range::new(Some(1), Some(3)).bound_range(&Range::new(Some(2), Some(4))),
        Range::new(Some(2), Some(3))
    );
    assert_eq!(
        Range::new(Some(1), Some(3)).bound_range(&Range::new(None, Some(4))),
        Range::new(Some(1), Some(3))
    );
    assert_eq!(
        Range::new(Some(1), None).bound_range(&Range::new(None, Some(4))),
        Range::new(Some(1), Some(4))
    );
    assert_eq!(
        Range::new(None, None).bound_range(&Range::new(None, Some(4))),
        Range::new(None, Some(4))
    );
    assert_eq!(
        Range::new(None, Some(3)).bound_range(&Range::new(Some(5), None)),
        Range::new(Some(3), Some(3))
    );
    assert_eq!(
        Range::new(Some(3), None).bound_range(&Range::new(Some(-2), Some(1))),
        Range::new(Some(3), Some(3))
    );
}