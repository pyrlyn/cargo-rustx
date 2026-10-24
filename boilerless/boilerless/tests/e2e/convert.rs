//! E2E `convert`: an id newtype converting to and from its raw value.
//! Manual boilerplate: two `From` impls.
//! Macro solution: `#[boil(convert)]`.
//! Checks: both conversions agree with the manual version.

use boilerless::Boil;

// ---- manual boilerplate ----
#[derive(Debug, PartialEq, Clone, Copy)]
struct ManualId(u64);
impl From<u64> for ManualId { fn from(v: u64) -> Self { Self(v) } }
impl From<ManualId> for u64 { fn from(v: ManualId) -> Self { v.0 } }

// ---- macro solution ----
#[derive(Boil, Debug, PartialEq, Clone, Copy)]
#[boil(convert)]
struct Id(u64);

fn takes_id(id: impl Into<Id>) -> u64 { id.into().into() }

fn main() {
    for raw in [0u64, 7, u64::MAX] {
        assert_eq!(ManualId::from(raw).0, Id::from(raw).0);
        assert_eq!(u64::from(ManualId(raw)), u64::from(Id(raw)));
        assert_eq!(takes_id(raw), raw);
    }
}
