// SPDX-License-Identifier: GPL-2.0-or-later

//! This module is intended for handling affine maps, such as
//!     --map abs:z abs:z:30-4x+d

use std::i32;

use crate::error::ArgumentError;
use crate::event::Event;
use crate::capability::Capability;
use crate::range::Interval;

#[cfg(test)]
use crate::range::Set;

#[derive(Clone, Copy, Debug)]
pub struct AffineFactor {
    absolute: f64,
    relative: f64,
    addition: f64,
}

impl AffineFactor {
    pub fn merge(&self, mut event: Event) -> Event {
        let absolute_factor = self.absolute * f64::from(event.value);
        // The following rounding is specially designed to avoid accumulating rounding
        // errors in cases like `--map abs:x rel:x:d`.
        let relative_factor =
            (f64::from(event.value) * self.relative).floor()
            - (f64::from(event.previous_value) * self.relative).floor();
        
        event.value = (
            (absolute_factor + self.addition).trunc() + relative_factor
        ) as i32;

        event
    }

    pub fn merge_cap(&self, cap: Capability) -> Capability {
        let new_values = cap.values.map(|interval| {
            let min: f64 = interval.min.into();
            let max: f64 = interval.max.into();
    
            let trunc_boundaries = (
                (mul_zero(min, self.absolute) + self.addition).trunc(),
                (mul_zero(max, self.absolute) + self.addition).trunc(),
            );
    
            let relative_span = mul_zero(self.relative, max-min);
    
            // In case the relative factor is nonzero and the range is unbounded
            // on one end, then the following list will contain NaNs. In that case,
            // the range of events is everything.
            let possible_boundaries: [f64; 4] = [
                trunc_boundaries.0 - relative_span, trunc_boundaries.0 + relative_span,
                trunc_boundaries.1 - relative_span, trunc_boundaries.1 + relative_span,
            ];
    
            let new_interval = if IntoIterator::into_iter(possible_boundaries).any(f64::is_nan) {
                Interval::new(None, None)
            } else {
                let lower_end = IntoIterator::into_iter(possible_boundaries).reduce(f64::min);
                let upper_end = IntoIterator::into_iter(possible_boundaries).reduce(f64::max);
        
                Interval::spanned_between(
                    to_i32_or(lower_end, i32::MIN),
                    to_i32_or(upper_end, i32::MAX),
                )
            };

            Some(new_interval)
        });

        cap.with_values(new_values)
    }

    /// Returns Some(value) if this factor can be seen as a simple constant.
    pub fn as_constant(&self) -> Option<f64> {
        if self.absolute == 0.0 && self.relative == 0.0 {
            Some(self.addition)
        } else {
            None
        }
    }
}

/// A multiplication functions where 0*anything=0.
/// This helps avoiding 0*Infinity resulting in NaN.
fn mul_zero(x: f64, y: f64) -> f64 {
    if x == 0.0 || y == 0.0 {
        0.0
    } else {
        x * y
    }
}

/// Returns the default value if the source is None or NaN. Otherwise casts the source to 32.
fn to_i32_or(source: Option<f64>, default: i32) -> i32 {
    let source = match source {
        Some(value) => value,
        None => return default,
    };

    if source.is_nan() {
        return default;
    }

    source as i32
}

struct Component {
    factor: f64,
    variable: Variable,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Variable {
    Value,
    Delta,
    One,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Sign {
    Positive,
    Negative,
}

enum Part {
    Sign(Sign),
    Numeric(Vec<char>),
    Variable(Variable),
}

fn lex_to_parts(source: &str) -> Result<Vec<Part>, ArgumentError> {
    let mut parts = Vec::new();
    if source.is_empty() {
        return Ok(parts);
    }

    for character in source.chars() {
        match character {
            '-' => parts.push(Part::Sign(Sign::Negative)),
            '+' => parts.push(Part::Sign(Sign::Positive)),
            '0' ..= '9' | '.' => {
                if let Some(Part::Numeric(vector)) = parts.last_mut() {
                    vector.push(character);
                } else {
                    parts.push(Part::Numeric(vec![character]));
                }
            },
            'x' => parts.push(Part::Variable(Variable::Value)),
            'd' => parts.push(Part::Variable(Variable::Delta)),
            _ => return Err(ArgumentError::new(format!("Invalid character: {}", character)))
        }
    }
    
    Ok(parts)
}

fn lex_to_components(source: &str) -> Result<Vec<Component>, ArgumentError> {
    let mut parts = lex_to_parts(source)?;
    
    // Add implicit first sign.
    match parts.first() {
        Some(Part::Sign(_)) => (),
        Some(_) => parts.insert(0, Part::Sign(Sign::Positive)),
        None => return Err(ArgumentError::new("Empty value.")),
    }

    let mut components: Vec<Component> = Vec::new();
    let mut parts_iter = parts.into_iter().peekable();
    loop {
        let sign = match parts_iter.next() {
            Some(Part::Sign(sign)) => sign,
            None => break,
            _ => return Err(ArgumentError::new("Expected sign, found something else.")),
        };
        let (numeric, variable) = match parts_iter.next() {
            Some(Part::Variable(variable)) => (vec!['1'], variable),
            Some(Part::Numeric(numeric)) => (numeric, match parts_iter.peek() {
                Some(&Part::Variable(variable)) => {
                    parts_iter.next();
                    variable
                },
                _ => Variable::One,
            }),
            _ => return Err(ArgumentError::new("Invalid expression.")),
        };

        let numeric_str = numeric.into_iter().collect::<String>();
        let number = match variable {
            Variable::One => numeric_str.parse::<i32>()
                .map_err(|_| ArgumentError::new("Cannot parse factor as integer."))?
                as f64,
            _ => numeric_str.parse::<f64>()
                .map_err(|_| ArgumentError::new("Cannot parse factor as number."))?,
        };

        let factor = match sign {
            Sign::Positive => number,
            Sign::Negative => -number,
        };
        
        components.push(Component { factor, variable });
    }

    Ok(components)
}

pub fn parse_affine_factor(source: &str) -> Result<AffineFactor, ArgumentError> {
    let components = lex_to_components(source)?;
    let mut result = AffineFactor {
        absolute: 0.0,
        relative: 0.0,
        addition: 0.0,
    };

    for component in components {
        match component.variable {
            Variable::Value => result.absolute += component.factor,
            Variable::Delta => result.relative += component.factor,
            Variable::One   => result.addition += component.factor,
        }
    }

    Ok(result)
}

#[test]
fn unittest() {
    let domain = crate::domain::get_unique_domain();
    let get_test_event = |value, previous_value| crate::event::Event {
        value, previous_value, domain,
        code: crate::event::EventCode::new(crate::event::EventType::new(1), 1),
        namespace: crate::event::Namespace::User,
    };
    let get_test_cap = |value_range| crate::capability::Capability {
        domain, values: Set::from(value_range),
        code: crate::event::EventCode::new(crate::event::EventType::new(1), 1),
        namespace: crate::event::Namespace::User,
        abs_meta: None,
    };

    assert_eq!(
        parse_affine_factor("1").unwrap().merge(get_test_event(7, 13)),
        get_test_event(1, 13),
    );
    assert_eq!(
        parse_affine_factor("2x+1").unwrap().merge(get_test_event(7, 13)),
        get_test_event(15, 13),
    );
    assert_eq!(
        parse_affine_factor("-2.5x+5").unwrap().merge(get_test_event(8, 13)),
        get_test_event(-15, 13),
    );
    assert_eq!(
        parse_affine_factor("d+x").unwrap().merge(get_test_event(7, 13)),
        get_test_event(1, 13),
    );
    assert_eq!(
        parse_affine_factor("-d+x").unwrap().merge(get_test_event(7, 13)),
        get_test_event(13, 13),
    );
    assert_eq!(
        parse_affine_factor("5+0x").unwrap().merge(get_test_event(7, 13)),
        get_test_event(5, 13),
    );

    assert_eq!(
        parse_affine_factor("-d+x+1").unwrap().merge_cap(get_test_cap(Interval::new(-2, 5))),
        get_test_cap(Interval::new(-8, 13)),
    );
    assert_eq!(
        parse_affine_factor("-d+x+1").unwrap().merge_cap(get_test_cap(Interval::new(None, 5))),
        get_test_cap(Interval::new(None, None)),
    );
    assert_eq!(
        parse_affine_factor("-d+x+1").unwrap().merge_cap(get_test_cap(Interval::new(-2, None))),
        get_test_cap(Interval::new(None, None)),
    );
    assert_eq!(
        parse_affine_factor("-x").unwrap().merge_cap(get_test_cap(Interval::new(-2, 5))),
        get_test_cap(Interval::new(-5, 2)),
    );
    assert_eq!(
        parse_affine_factor("-x").unwrap().merge_cap(get_test_cap(Interval::new(None, 7))),
        get_test_cap(Interval::new(-7, None)),
    );
    assert_eq!(
        parse_affine_factor("8").unwrap().merge_cap(get_test_cap(Interval::new(-2, 5))),
        get_test_cap(Interval::new(8, 8)),
    );
    assert_eq!(
        parse_affine_factor("8").unwrap().merge_cap(get_test_cap(Interval::new(None, None))),
        get_test_cap(Interval::new(8, 8)),
    );
    

    assert!(parse_affine_factor("z").is_err());
    assert!(parse_affine_factor("--x").is_err());
    assert!(parse_affine_factor("x3").is_err());
}