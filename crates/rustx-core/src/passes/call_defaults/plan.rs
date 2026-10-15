//! What a call becomes for one candidate fn: a slot per parameter, or the problems that stop it.
//!
//! Plans are plain data so the walk can compare the plans of every candidate a name resolves
//! to: equal plans are rewritten once, different ones are ambiguous.

use super::collect::Shape;

/// Where the value of one parameter comes from; indices point into the call's arguments.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Slot {
    /// A parameter without a default: the argument as written.
    Plain(usize),
    /// A defaulted parameter the call gives: `Some(argument)`.
    Some(usize),
    /// A defaulted parameter the call leaves out: `None`.
    None,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub(super) enum Problem {
    /// More arguments than parameters; `max` is also the index of the first one too many.
    TooMany { max: usize },
    /// A parameter without a default that the call leaves out.
    Missing { label: String },
}

pub(super) type Plan = Result<Vec<Slot>, Vec<Problem>>;

/// The plan for a call with `given` positional arguments.
pub(super) fn plan(shape: &Shape, given: usize) -> Plan {
    let mut problems = Vec::new();
    let max = shape.params.len();
    if given > max {
        problems.push(Problem::TooMany { max });
    }
    let slots = shape
        .params
        .iter()
        .enumerate()
        .map(|(at, param)| match (at < given, param.default) {
            (true, false) => Slot::Plain(at),
            (true, true) => Slot::Some(at),
            (false, true) => Slot::None,
            (false, false) => {
                problems.push(Problem::Missing {
                    label: param.label.clone(),
                });
                Slot::None
            }
        })
        .collect();
    if problems.is_empty() {
        Ok(slots)
    } else {
        Err(problems)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::passes::call_defaults::collect::Param;

    fn shape(params: &[(&str, bool)]) -> Shape {
        Shape {
            params: params
                .iter()
                .map(|&(name, default)| Param {
                    name: Some(name.to_owned()),
                    label: name.to_owned(),
                    default,
                })
                .collect(),
        }
    }

    #[test]
    fn given_defaults_are_wrapped_and_omitted_ones_filled() {
        let connect = shape(&[("host", false), ("port", true), ("timeout", true)]);
        assert_eq!(
            plan(&connect, 1),
            Ok(vec![Slot::Plain(0), Slot::None, Slot::None])
        );
        assert_eq!(
            plan(&connect, 2),
            Ok(vec![Slot::Plain(0), Slot::Some(1), Slot::None])
        );
    }

    #[test]
    fn too_many_and_missing_arguments_are_problems() {
        let connect = shape(&[("host", false), ("port", true)]);
        assert_eq!(plan(&connect, 3), Err(vec![Problem::TooMany { max: 2 }]));
        assert_eq!(
            plan(&connect, 0),
            Err(vec![Problem::Missing {
                label: "host".to_owned()
            }])
        );
    }
}
