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
    /// A named argument (at index `arg`) that names no parameter.
    Unknown { arg: usize },
    /// A named argument for a parameter without a default, which goes by position.
    NotDefaulted { arg: usize },
    /// A named argument for a parameter that already has a value.
    Duplicate { arg: usize },
    /// A positional argument after a named one.
    PositionalAfterNamed { arg: usize },
}

pub(super) type Plan = Result<Vec<Slot>, Vec<Problem>>;

/// The plan for a call whose arguments have these names (`None` for a positional one).
///
/// Positional arguments fill the parameters in order; named ones the parameter of that name,
/// which must have a default, so a name never moves a required argument.
pub(super) fn plan(shape: &Shape, names: &[Option<String>]) -> Plan {
    let mut problems = Vec::new();
    let max = shape.params.len();
    let given = names.iter().take_while(|name| name.is_none()).count();
    if given > max {
        problems.push(Problem::TooMany { max });
    }
    let mut source: Vec<Option<usize>> = (0..max).map(|at| (at < given).then_some(at)).collect();
    for (arg, name) in names.iter().enumerate().skip(given) {
        let Some(name) = name else {
            problems.push(Problem::PositionalAfterNamed { arg });
            continue;
        };
        let param = shape
            .params
            .iter()
            .position(|param| param.name.as_ref() == Some(name));
        match param {
            None => problems.push(Problem::Unknown { arg }),
            Some(at) if !shape.params[at].default => problems.push(Problem::NotDefaulted { arg }),
            Some(at) if source[at].is_some() => problems.push(Problem::Duplicate { arg }),
            Some(at) => source[at] = Some(arg),
        }
    }
    let slots = shape
        .params
        .iter()
        .zip(source)
        .map(|(param, source)| match (source, param.default) {
            (Some(arg), false) => Slot::Plain(arg),
            (Some(arg), true) => Slot::Some(arg),
            (None, true) => Slot::None,
            (None, false) => {
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

    fn names(names: &[Option<&str>]) -> Vec<Option<String>> {
        names.iter().map(|name| name.map(str::to_owned)).collect()
    }

    fn positional(count: usize) -> Vec<Option<String>> {
        vec![None; count]
    }

    #[test]
    fn given_defaults_are_wrapped_and_omitted_ones_filled() {
        let connect = shape(&[("host", false), ("port", true), ("timeout", true)]);
        assert_eq!(
            plan(&connect, &positional(1)),
            Ok(vec![Slot::Plain(0), Slot::None, Slot::None])
        );
        assert_eq!(
            plan(&connect, &positional(2)),
            Ok(vec![Slot::Plain(0), Slot::Some(1), Slot::None])
        );
    }

    #[test]
    fn too_many_and_missing_arguments_are_problems() {
        let connect = shape(&[("host", false), ("port", true)]);
        assert_eq!(
            plan(&connect, &positional(3)),
            Err(vec![Problem::TooMany { max: 2 }])
        );
        assert_eq!(
            plan(&connect, &positional(0)),
            Err(vec![Problem::Missing {
                label: "host".to_owned()
            }])
        );
    }

    #[test]
    fn named_arguments_fill_their_parameter_in_declaration_order() {
        let connect = shape(&[("host", false), ("port", true), ("timeout", true)]);
        assert_eq!(
            plan(&connect, &names(&[None, Some("timeout")])),
            Ok(vec![Slot::Plain(0), Slot::None, Slot::Some(1)])
        );
        assert_eq!(
            plan(&connect, &names(&[None, Some("timeout"), Some("port")])),
            Ok(vec![Slot::Plain(0), Slot::Some(2), Slot::Some(1)])
        );
    }

    #[test]
    fn misused_names_are_problems() {
        let connect = shape(&[("host", false), ("port", true)]);
        let problems = |args: &[Option<&str>]| plan(&connect, &names(args)).err();
        assert_eq!(
            problems(&[None, Some("prot")]),
            Some(vec![Problem::Unknown { arg: 1 }])
        );
        assert_eq!(
            problems(&[Some("host")]),
            Some(vec![
                Problem::NotDefaulted { arg: 0 },
                Problem::Missing {
                    label: "host".to_owned()
                }
            ])
        );
        assert_eq!(
            problems(&[None, None, Some("port")]),
            Some(vec![Problem::Duplicate { arg: 2 }])
        );
        assert_eq!(
            problems(&[None, Some("port"), None]),
            Some(vec![Problem::PositionalAfterNamed { arg: 2 }])
        );
    }
}
