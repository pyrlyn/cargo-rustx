# Walls of `#[derive(..)]`

## As is (Rust)

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct UserId(pub u64);
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct OrderId(pub u64);
```

## Desired (sugar)

```rust
derive Id = Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord;

#[derive(Id)] pub struct UserId(pub u64);
#[derive(Id)] pub struct OrderId(pub u64);
```

## Implementation

A `derive Name = A, B, C;` item declares a preset inside `sugar!`. In every `#[derive(..)]` of the same input, a preset name expands to its list, and the result is deduplicated while keeping order, so presets mix with ordinary derives (`#[derive(Id, serde::Serialize)]`). A preset that shadows a std derive name (`Debug`, `Clone`, ...) is a compile error.

## Detailed example

### As is (valid Rust)

```rust
use std::collections::{BTreeMap, HashSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct UserId(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct OrderId(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Status {
    New,
    Paid,
    Shipped,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct Address {
    pub city: String,
    pub street: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Order {
    pub id: OrderId,
    pub user: UserId,
    pub status: Status,
    pub ship_to: Address,
}

pub fn index(orders: &[Order]) -> (BTreeMap<OrderId, UserId>, HashSet<Address>) {
    let by_id = orders.iter().map(|o| (o.id, o.user)).collect();
    let cities = orders
        .iter()
        .filter(|o| o.status >= Status::Paid)
        .map(|o| o.ship_to.clone())
        .collect();
    (by_id, cities)
}
```

### Desired (cargo-rustx)

```rust
// Function-like entry point: the body is not valid Rust, so #[sugar] on an item cannot be used
sugar! {
mod orders {
    use std::collections::{BTreeMap, HashSet};

    derive Id = Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord;
    derive Value = Debug, Clone, PartialEq, Eq, Hash;

    #[derive(Id)] pub struct UserId(pub u64);
    #[derive(Id)] pub struct OrderId(pub u64);
    #[derive(Id)] pub enum Status { New, Paid, Shipped }

    #[derive(Value, Default)]
    pub struct Address { pub city: String, pub street: String }

    #[derive(Value)]
    pub struct Order { pub id: OrderId, pub user: UserId, pub status: Status, pub ship_to: Address }

    pub fn index(orders: &[Order]) -> (BTreeMap<OrderId, UserId>, HashSet<Address>) {
        let by_id = orders.iter().map(|o| (o.id, o.user)).collect();
        let cities = orders
            .iter()
            .filter(|o| o.status >= Status::Paid)
            .map(|o| o.ship_to.clone())
            .collect();
        (by_id, cities)
    }
}
}
```

### What changed

- Five copies of a seven- or eight-name derive list became two named presets. The intent ("an id", "a value") reads at a glance.
- Adding a derive to every id type (for example `serde::Serialize`) is a one-line change to the preset.
- Presets are local to one `sugar!` input (no cross-item info). Sharing them across files needs the file-level transpiler (M9).
