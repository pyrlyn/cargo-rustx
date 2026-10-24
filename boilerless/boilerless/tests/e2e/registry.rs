//! E2E `registry` (rare-but-frequent): enumerate all plugin kinds.
//! Manual boilerplate: a hand-maintained `ALL` array, count and index match.
//! Macro solution: `#[boil(registry)]`.
//! Checks: same order, count and indices; combined with `enum_str` for names.

use boilerless::Boil;

// ---- manual boilerplate ----
#[derive(Debug, PartialEq, Clone, Copy)]
enum ManualKind { Http, Grpc, Cron }
impl ManualKind {
    const ALL: &'static [Self] = &[Self::Http, Self::Grpc, Self::Cron];
    fn index(&self) -> usize { match self { Self::Http => 0, Self::Grpc => 1, Self::Cron => 2 } }
}

// ---- macro solution ----
#[derive(Boil, Debug, PartialEq, Clone, Copy)]
#[boil(registry, enum_str)]
enum Kind { Http, Grpc, Cron }

fn main() {
    assert_eq!(ManualKind::ALL.len(), Kind::COUNT);
    for (m, g) in ManualKind::ALL.iter().zip(Kind::ALL) {
        assert_eq!(m.index(), g.index());
        assert_eq!(format!("{m:?}"), g.variant_name());
        assert_eq!(g.to_string().parse::<Kind>(), Ok(*g));
    }
}
