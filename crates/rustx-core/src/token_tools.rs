//! Shared syntax helpers for tooling that works on unexpanded rustx source.
//!
//! The formatter and expansion passes use the same scanners so new syntax does not require
//! a second parser. Indices refer to a flat sequence; delimiter groups are single tokens.

pub use crate::tokens::angle::{angle_depth, matching_close};
pub use crate::tokens::params::{argument_end, parameter_list};
pub use crate::tokens::syntax::conditional_attributes;
