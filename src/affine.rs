// SPDX-License-Identifier: GPL-2.0-or-later

//! This module is intended for handling affine maps, such as
//!     --map abs:z abs:z:30-4x+d

use crate::error::ArgumentError;
use crate::event::Event;
use crate::capability::Capability;
use crate::range::Range;

pub struct AffineFactor {
    components: Vec<Component>,
}

impl AffineFactor {
    pub fn merge(&self, mut event: Event) -> Event {
        let mut new_value: f64 = 0.0;
        for component in &self.components {
            match component.variable {
                // TODO: Think some more about the following rounding method.
                Variable::One => new_value += component.factor,
                Variable::Value => new_value += component.factor * f64::from(event.value),
                Variable::Delta => {
                    new_value += (f64::from(event.value) * component.factor).floor()
                                 - (f64::from(event.previous_value) * component.factor).floor();
                },
            }
        }
        event.value = new_value.trunc() as i32;

        event
    }

    pub fn merge_cap(&self, mut cap: Capability) -> Capability {
        // TODO: CRITICAL: Properly infering the range is still unimplemented.
        cap.value_range = Range::new(None, None);
        cap
    }
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

        let number = numeric.into_iter().collect::<String>().parse::<f64>()
            .map_err(|_| ArgumentError::new("Cannot parse factor as number."))?;
        let factor = match sign {
            Sign::Positive => number,
            Sign::Negative => -number,
        };
        
        components.push(Component { factor, variable });
    }

    Ok(components)
}

pub fn parse_affine_factor(source: &str) -> Result<AffineFactor, ArgumentError> {
    // TODO: BEFORE-STABILIZE: Forbid multiple copies of the same variable?
    // 0.1d + 0.25d may mean something different from 0.35d.
    Ok(AffineFactor {
        components: lex_to_components(source)?
    })
}

#[test]
fn unittest() {
    let domain = crate::domain::get_unique_domain();
    let get_test_event = |value, previous_value| crate::event::Event {
        value, previous_value, domain,
        code: crate::event::EventCode::new(crate::event::EventType::new(1), 1),
        namespace: crate::event::Namespace::User,
        flags: crate::event::EventFlags::empty(),
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
        parse_affine_factor("-2.5x+0.5").unwrap().merge(get_test_event(7, 13)),
        get_test_event(-17, 13),
    );
    assert_eq!(
        parse_affine_factor("d+x").unwrap().merge(get_test_event(7, 13)),
        get_test_event(1, 13),
    );
    assert_eq!(
        parse_affine_factor("-d+x").unwrap().merge(get_test_event(7, 13)),
        get_test_event(13, 13),
    );

    assert!(parse_affine_factor("z").is_err());
    assert!(parse_affine_factor("--x").is_err());
    assert!(parse_affine_factor("x3").is_err());
}