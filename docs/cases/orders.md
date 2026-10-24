# Orders

Parsing an order id, rejecting an empty sku and defaulting the quantity. Ordinary Rust writes the newtype's `Display` and `FromStr`, the error's `Display`, `Error` and `From`, the derive list, the getters, and an `Option` for the defaulted quantity. [`#[newtype]`](../guide/newtype.md), [`derive` presets](../guide/derive-presets.md), [`#[data]`](../guide/data.md), [`#[display]`](../guide/from-display.md), [`throws`](../guide/throws.md), [`into`](../guide/into-params.md) and [parameter defaults](../guide/param-defaults.md) collapse that.

## Plain Rust

```rust
use std::fmt;
use std::str::FromStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct OrderId(u64);

impl OrderId {
    fn new(inner: u64) -> Self {
        Self(inner)
    }
}

impl fmt::Display for OrderId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, f)
    }
}

impl FromStr for OrderId {
    type Err = std::num::ParseIntError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self::new(s.parse()?))
    }
}

#[derive(Debug)]
struct Order {
    id: OrderId,
    sku: String,
    qty: u16,
}

impl Order {
    fn id(&self) -> &OrderId {
        &self.id
    }
    fn sku(&self) -> &str {
        &self.sku
    }
    fn qty(&self) -> u16 {
        self.qty
    }
}

#[derive(Debug)]
enum OrderError {
    Id(std::num::ParseIntError),
    Empty,
}

impl fmt::Display for OrderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Id(err) => write!(f, "bad id: {err}"),
            Self::Empty => f.write_str("empty sku"),
        }
    }
}

impl std::error::Error for OrderError {}

impl From<std::num::ParseIntError> for OrderError {
    fn from(err: std::num::ParseIntError) -> Self {
        Self::Id(err)
    }
}

fn parse_order(id: &str, sku: impl Into<String>, qty: Option<u16>) -> Result<Order, OrderError> {
    let sku = sku.into();
    let qty = qty.unwrap_or(1);
    if sku.is_empty() {
        return Err(OrderError::Empty);
    }
    Ok(Order { id: id.parse()?, sku, qty })
}

fn demo() -> (String, String, u16, String, String) {
    let order = parse_order("12", "bolt", None).unwrap();
    let named = parse_order("9", "nail", Some(4)).unwrap();
    (
        order.id().to_string(),
        order.sku().to_string(),
        named.qty(),
        parse_order("x", "bolt", None).unwrap_err().to_string(),
        parse_order("1", "", None).unwrap_err().to_string(),
    )
}

let (id, sku, qty, bad_id, empty) = demo();
assert_eq!((id.as_str(), sku.as_str(), qty), ("12", "bolt", 4));
assert!(bad_id.starts_with("bad id: "));
assert_eq!(empty, "empty sku");
```

## With rustx

```rust
rustx::sugar! {
    use std::num::ParseIntError;

    derive Id = Debug, Clone, Copy, PartialEq, Eq;

    #[newtype(display, from_str)]
    #[derive(Id)]
    struct OrderId(u64);

    #[data]
    #[derive(Debug)]
    struct Order {
        id: OrderId,
        sku: String,
        qty: u16,
    }

    #[derive(Debug)]
    enum OrderError {
        #[display("bad id: {0}")]
        Id(from ParseIntError),
        #[display("empty sku")]
        Empty,
    }

    fn parse_order(id: &str, sku: into String, qty: u16 = 1) -> Order throws OrderError {
        if sku.is_empty() {
            throw OrderError::Empty;
        }
        Order { id: id.parse<OrderId>()?, sku, qty }
    }

    fn demo() -> (String, String, u16, String, String) {
        let order = parse_order("12", "bolt").unwrap();
        let named = parse_order("9", "nail", qty: 4).unwrap();
        (
            order.id().to_string(),
            order.sku().to_string(),
            named.qty(),
            parse_order("x", "bolt").unwrap_err().to_string(),
            parse_order("1", "").unwrap_err().to_string(),
        )
    }
}

let (id, sku, qty, bad_id, empty) = demo();
assert_eq!((id.as_str(), sku.as_str(), qty), ("12", "bolt", 4));
assert!(bad_id.starts_with("bad id: "));
assert_eq!(empty, "empty sku");
```
