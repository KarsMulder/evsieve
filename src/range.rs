// SPDX-License-Identifier: GPL-2.0-or-later

use std::convert::TryFrom;
use std::convert::TryInto;
use std::i32;

/// A bound for the values of an Event's current value or previous value.
/// Represents a closed interval.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub struct Interval {
    /// The values min and max are inclusive bounds.
    /// Always be VERY careful with + and - around these numbers. Those operations can easily overflow!
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

/// Represents a subset of the interval [i32::MIN, i32::MAX].
/// Of course any such set could be represented using 2^28 bytes of memory, but for efficiency,
/// we represent such sets as unions of contiguous intervals, e.g. [-5, -2] U [7, 12] U [18, i32::MAX].
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct Set {
    /// Invariants to be upheld:
    /// 1. All intervals are disjoint.
    /// 2. The intervals should be ordered, i.e. if i<j then `intervals[i].max < intervals[j].min``
    intervals: Vec<Interval>,
}

impl From<Interval> for Set {
    fn from(value: Interval) -> Self {
        Self {
            intervals: vec![value],
        }
    }
}

impl Set {
    pub fn intersect(&self, other: &Set) -> Set {
        let mut intervals_out = Vec::new();
        let pair_iter = IntervalPairIterator::new(self.intervals.iter().copied(), other.intervals.iter().copied());

        for (interval_1, interval_2) in pair_iter {
            if let Some(intersection) = interval_1.intersect(&interval_2) {
                intervals_out.push(intersection);
            }
        }
        Set::from_unordered_intervals(intervals_out)
    }

    pub fn union(&self, other: &Set) -> Set {
        let mut intervals_out = Vec::with_capacity(self.intervals.len() + other.intervals.len());
        intervals_out.extend(self.intervals.iter().copied());
        intervals_out.extend(other.intervals.iter().copied());
        Set::from_unordered_intervals(intervals_out)
    }

    /// Returns [i32::MIN, i32::MAX] \ self.
    pub fn complement(&self) -> Set {

        let (first_interval, last_interval) = match (self.intervals.first(), self.intervals.last()) {
            (Some(first), Some(last)) => (first, last),
            _ =>  {
                // If first() and last() return None, then the intervals vector is empty, which means that this
                // is the empty set and the complement is the universe set [i32::MIN, i32::MAX].
                return Set {
                    intervals: vec![Interval::new(i32::MIN, i32::MAX)]
                }
            }
        };

        let mut result = Vec::new();

        if first_interval.min > i32::MIN {
            // first_interval.min has been checked to be greater than i32::MIN, therefore we should be able to
            // subtract one from it.
            result.push(Interval::new(i32::MIN, first_interval.min.checked_sub(1).unwrap()));
        }
        for interval_pair in self.intervals.windows(2) {
            let [interval_a, interval_b] = match interval_pair {
                [a, b] => [a, b],
                _ => panic!("slice::windows(2) did return a window that did not contain two elements."),
            };

            if interval_b.min > interval_a.max.saturating_add(1) {
                result.push(Interval::new(
                    // Adding and subtracting should be fine because if either of those additions/subtractions would overflow,
                    // the condition interval_b.min > interval_a.max+1 couldn't be true.
                    interval_a.max.checked_add(1).unwrap(),
                    interval_b.min.checked_sub(1).unwrap()
                ));
            }
        }
        if last_interval.max < i32::MAX {
            result.push(Interval::new(last_interval.max.checked_add(1).unwrap(), i32::MAX));
        }


        Set { intervals: result }
    }

    /// In mathematical notation, computes self \ other.
    pub fn setminus(&self, other: &Set) -> Set {
        self.intersect(&other.complement())
    }

    // Returns an interval that contains all values in this set.
    // Returns None if this is the empty set.
    pub fn spanning_interval(&self) -> Option<Interval> {
        if let (Some(first), Some(last)) = (self.intervals.first(), self.intervals.last()) {
            Some(Interval::new(first.min, last.max))
        } else {
            None
        }
    }

    /// Returns the empty set.
    pub fn empty() -> Set {
        Set { intervals: Vec::new() }
    }

    /// Tells you whether this is the empty set.
    pub fn is_empty(&self) -> bool {
        self.intervals.is_empty()
    }

    /// Applies a function to each interval in this set. That function may return zero or more intervals.
    /// Returns the set that is the union of all returned intervals.
    pub fn map<T: IntoIterator<Item=Interval>>(&self, function: impl Fn(Interval) -> T) -> Set {
        Set::from_unordered_intervals(self.intervals.iter().copied().flat_map(function).collect())
    }

    /// Creates a Set from intervals that may or may not be ordered and may or may not be disjoint.
    pub fn from_unordered_intervals(mut intervals: Vec<Interval>) -> Set {
        // Sort the intervals.
        intervals.sort_unstable_by_key(|interval| interval.max);

        // Merge overlapping intervals together, e.g. [1, 5] U [3, 7] -> [1, 7]
        let mut merged_intervals: Vec<Interval> = Vec::new();

        for mut interval in intervals {
            while let Some(last_interval) = merged_intervals.last() {
                if last_interval.max >= interval.min.saturating_sub(1) {
                    // Merge the last interval with the current interval.
                    interval.min = std::cmp::min(interval.min, last_interval.min);
                    merged_intervals.pop();
                } else {
                    break;
                }
            }
            merged_intervals.push(interval);
        }

        Set { intervals: merged_intervals }
    }
}

/// Generates pairs of intervals (interval_1, interval_2). Consecutively generated pairs will have exactly one
/// interval different. The interval that differs will always be the one whose maximum value was the lowest.
/// Unless the one with the lowest maximum value has reached end of iteration, then the other will change.
/// 
/// For example, if the first iterator yields [1,2], [3, 4] and the second iterator yields [2, 3], [5, 7] then
/// the pair iterator will yield ([1, 2], [2, 3]), ([3, 4], [2, 3]), ([3, 4], [5, 7])
struct IntervalPairIterator<T: Iterator<Item=Interval>> {
    interval_iter_1: T,
    interval_iter_2: T,
    next_interval_1: Option<Interval>,
    next_interval_2: Option<Interval>,
}

impl<T: Iterator<Item=Interval>>  IntervalPairIterator<T> {
    fn new(interval_iter_1: impl IntoIterator<IntoIter = T>, interval_iter_2: impl IntoIterator<IntoIter = T>) -> Self {
        let mut interval_iter_1 = interval_iter_1.into_iter();
        let mut interval_iter_2 = interval_iter_2.into_iter();
        let next_interval_1 = interval_iter_1.next();
        let next_interval_2 = interval_iter_2.next();
        Self { interval_iter_1, interval_iter_2, next_interval_1, next_interval_2 }
    }
}

impl<T: Iterator<Item=Interval>> Iterator for IntervalPairIterator<T> {
    type Item = (Interval, Interval);

    fn next(&mut self) -> Option<Self::Item> {
        let interval_1 = self.next_interval_1?;
        let interval_2 = self.next_interval_2?;

        // Figure out which of the two interval iterators we want to advance.
        let (primary_iter, primary_next, secondary_iter, secondary_next) = if interval_1.max < interval_2.max {
            (&mut self.interval_iter_1, &mut self.next_interval_1, &mut self.interval_iter_2, &mut self.next_interval_2)
        } else {
            (&mut self.interval_iter_2, &mut self.next_interval_2, &mut self.interval_iter_1, &mut self.next_interval_1)
        };

        // Advance the primary iterator unless it has reached the end of its iterations, in which case the secondary
        // iterator must advance.
        match primary_iter.next() {
            Some(value) => *primary_next = Some(value),
            None => match secondary_iter.next() {
                Some(value) => *secondary_next = Some(value),
                None => (*primary_next, *secondary_next) = (None, None),
            }
        }

        Some((interval_1, interval_2))
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
fn test_interval_iterator() {
    assert_eq!(
        IntervalPairIterator::new(
            vec![Interval::new(1, 2), Interval::new(3, 4), Interval::new(5, 6), Interval::new(7, 8)],
            vec![Interval::new(2, 3), Interval::new(5, 7), Interval::new(11, 13), Interval::new(17, 19), Interval::new(23, 29)],
        ).collect::<Vec<_>>(),
        vec![
            (Interval::new(1, 2), Interval::new(2, 3)),
            (Interval::new(3, 4), Interval::new(2, 3)),
            (Interval::new(3, 4), Interval::new(5, 7)),
            (Interval::new(5, 6), Interval::new(5, 7)),
            (Interval::new(7, 8), Interval::new(5, 7)),
            (Interval::new(7, 8), Interval::new(11, 13)),
            (Interval::new(7, 8), Interval::new(17, 19)),
            (Interval::new(7, 8), Interval::new(23, 29)),
        ]
    );
}

#[test]
fn test_set() {
    assert_eq!(
        Set {
            intervals: vec![Interval::new(1, 2), Interval::new(3, 4), Interval::new(5, 6), Interval::new(7, 8)],
        }.intersect(&Set {
            intervals: vec![Interval::new(2, 3), Interval::new(5, 7), Interval::new(11, 13), Interval::new(17, 19), Interval::new(23, 29)]
        }).intervals,

        vec![Interval::new(2, 3), Interval::new(5, 7)]
    );

    assert_eq!(
        Set {
            intervals: vec![Interval::new(i32::MIN, -5), Interval::new(11, 20), Interval::new(30, i32::MAX)],
        }.intersect(&Set {
            intervals: vec![Interval::new(i32::MIN, 40), Interval::new(50, 60), Interval::new(100, i32::MAX)]
        }).intervals,

        vec![Interval::new(i32::MIN, -5), Interval::new(11, 20), Interval::new(30, 40), Interval::new(50, 60), Interval::new(100, i32::MAX)]
    );

    assert_eq!(
        Set {
            intervals: vec![Interval::new(1, 2), Interval::new(3, 4), Interval::new(5, 6), Interval::new(7, 8)],
        }.union(&Set {
            intervals: vec![Interval::new(2, 3), Interval::new(5, 7), Interval::new(11, 13), Interval::new(17, 19), Interval::new(23, 29)]
        }).intervals,

        vec![Interval::new(1, 8), Interval::new(11, 13), Interval::new(17, 19), Interval::new(23, 29)]
    );

    assert_eq!(
        Set {
            intervals: vec![Interval::new(i32::MIN, -5), Interval::new(11, 20), Interval::new(30, i32::MAX)],
        }.union(&Set {
            intervals: vec![Interval::new(i32::MIN, 40), Interval::new(50, 60), Interval::new(100, i32::MAX)]
        }).intervals,

        vec![Interval::new(i32::MIN, i32::MAX)]
    );

    
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
