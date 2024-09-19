// SPDX-License-Identifier: GPL-2.0-or-later

use std::convert::TryFrom;
use std::convert::TryInto;
use std::i32;

/// A bound for the values of an Event's current value or previous value.
/// Represents a closed interval.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub struct Interval {
    /// The values min and max are inclusive bounds.
    pub min: i32,
    pub max: i32,
}

impl Interval {
    pub fn new(min: impl Into<Option<i32>>, max: impl Into<Option<i32>>) -> Interval {
        Interval {
            min: match min.into() {
                Some(value) => value,
                None => i32::MIN,
            },
            max: match max.into() {
                Some(value) => value,
                None => i32::MAX,
            },
        }
    }

    /// Returns a range that lies between the two provided integers. The two integers
    /// do not need to be provided in ascending order.
    pub fn spanned_between(a: i32, b: i32) -> Interval {
        let max = std::cmp::max(a, b);
        let min = std::cmp::min(a, b);
        Interval { min, max }
    }

    /// Checks whether this Range contains a value.
    pub fn contains(&self, value: i32) -> bool {
        self.min <= value && value <= self.max
    }

    /// Returns the closest integer to value that lies within this Range.
    pub fn bound(&self, value: i32) -> i32 {
        std::cmp::max(self.min, std::cmp::min(self.max, value))
    }

    /// Returns the closest integer to value that lies within this Range.
    pub fn bound_f64(&self, value: f64) -> f64 {
        if value < self.min.into() {
            self.min.into()
        } else if value > self.max.into() {
            self.max.into()
        } else {
            value
        }
    }

    /// The maximum difference between two event values that can fall in this range, which is one less
    /// than the total amount of event values that can fall in this range.
    pub fn span(&self) -> u32 {
        let min: i32 = self.min;
        let max: i32 = self.max;
        let min_i64: i64 = min.into();
        let max_i64: i64 = max.into();
        // try_into() will never fail because (i32::MAX - i32::MIN) fits within an u32.
        (max_i64 - min_i64).try_into().unwrap()
    }

    /// A range that contains every possible difference between two event codes that fall in this range.
    pub fn delta_range(&self) -> Interval {
        Interval {
            min: i32::try_from(self.span()).map(|x| -x).unwrap_or(i32::MIN),
            max: i32::try_from(self.span()).unwrap_or(i32::MAX),
        }
    }

    /// Returns the range that would be generated if we bounded every value in the other range.
    pub fn bound_range(&self, other: &Interval) -> Interval {
        // If we overlap, every bounded value will lie in that overlapping.
        if let Some(intersection) = self.intersect(other) {
            intersection
        // Otherwise all values will be projected to a single point, depending on whether the
        // other range lies entirely above or below this range.
        } else if self.min > other.max {
            Interval { min: self.min, max: self.min }
        } else {
            Interval { min: self.max, max: self.max }
        }
    }

    /// Returns the largest range that is contained by both self and other.
    pub fn intersect(&self, other: &Interval) -> Option<Interval> {
        let max = std::cmp::min(self.max, other.max);
        let min = std::cmp::max(self.min, other.min);
        if min > max {
            None
        } else {
            Some(Interval {min, max})
        }
    }

    /// Returns true if some value lies in both this range and the other.
    pub fn intersects_with(&self, other: &Interval) -> bool {
        self.intersect(other).is_some()
    }

    /// Returns the smallest range that contains both self and other.
    /// We don't call this `union` because values that are in neither original range
    /// may show up in the merged range.
    pub fn merge(&self, other: &Interval) -> Interval {
        let min = std::cmp::min(self.min, other.min);
        let max = std::cmp::max(self.max, other.max);

        Interval { min, max }
    }

    /// Returns a range if there is a contiguous range that is the union of both of these.
    /// If such a range does not exist (e.g. there is empty space between them), returns None.
    pub fn try_union(&self, other: &Interval) -> Option<Interval> {
        if self.intersect(other).is_none() &&
           ! is_adjacent(self.max, other.min) &&
           ! is_adjacent(self.min, other.max)
        {
            return None;
        }

        Some(Interval {
            min: std::cmp::min(self.min, other.min),
            max: std::cmp::max(self.max, other.max),
        })
    }

    /// Tests whether this range is a subset of another range.
    pub fn is_subset_of(&self, other: &Interval) -> bool {
        self.intersect(other) == Some(*self)
    }

    /// Tests whether these ranges have no overlap.
    pub fn is_disjoint_with(&self, other: &Interval) -> bool {
        self.intersect(other).is_none()
    }
}

/// Returns true if x and y differ by exactly one.
fn is_adjacent(x: i32, y: i32) -> bool {
    // This function is written in a roundabout way to eliminate the possibility of integer overflow ocurring.
    if x < y {
        y == x + 1
    } else if y < x {
        x == y + 1
    } else {
        false
    }
}

#[test]
fn unittest() {
    // Intersection test
    assert_eq!(
        Interval::new(Some(1), Some(3)).intersect(&Interval::new(Some(2), Some(4))),
        Some(Interval::new(Some(2), Some(3)))
    );
    assert_eq!(
        Interval::new(Some(2), Some(4)).intersect(&Interval::new(Some(1), Some(3))),
        Some(Interval::new(Some(2), Some(3)))
    );
    assert_eq!(
        Interval::new(Some(1), Some(3)).intersect(&Interval::new(Some(5), Some(7))),
        None,
    );
    assert_eq!(
        Interval::new(Some(1), Some(3)).intersect(&Interval::new(Some(-4), Some(-2))),
        None,
    );
    assert_eq!(
        Interval::new(Some(1), Some(3)).intersect(&Interval::new(Some(-4), None)),
        Some(Interval::new(Some(1), Some(3))),
    );

    //Bounding tests.
    assert_eq!(
        Interval::new(Some(1), Some(3)).bound_range(&Interval::new(Some(2), Some(4))),
        Interval::new(Some(2), Some(3))
    );
    assert_eq!(
        Interval::new(Some(1), Some(3)).bound_range(&Interval::new(None, Some(4))),
        Interval::new(Some(1), Some(3))
    );
    assert_eq!(
        Interval::new(Some(1), None).bound_range(&Interval::new(None, Some(4))),
        Interval::new(Some(1), Some(4))
    );
    assert_eq!(
        Interval::new(None, None).bound_range(&Interval::new(None, Some(4))),
        Interval::new(None, Some(4))
    );
    assert_eq!(
        Interval::new(None, Some(3)).bound_range(&Interval::new(Some(5), None)),
        Interval::new(Some(3), Some(3))
    );
    assert_eq!(
        Interval::new(Some(3), None).bound_range(&Interval::new(Some(-2), Some(1))),
        Interval::new(Some(3), Some(3))
    );

    // Delta-range tests.
    assert_eq!(
        Interval::new(Some(3), Some(7)).delta_range(),
        Interval::new(Some(-4), Some(4))
    );
    assert_eq!(
        Interval::new(Some(-12), Some(2)).delta_range(),
        Interval::new(Some(-14), Some(14))
    );
    assert_eq!(
        Interval::new(None, Some(7)).delta_range(),
        Interval::new(None, None)
    );
    assert_eq!(
        Interval::new(None, None).delta_range(),
        Interval::new(None, None)
    );
}
