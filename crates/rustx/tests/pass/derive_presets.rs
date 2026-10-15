use std::collections::{BTreeMap, HashSet};

rustx::sugar! {
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

rustx::sugar! {
    derive Base = Debug, Clone;
    derive Id = Base, Copy, PartialEq;
    derive Key = Id, Eq, Hash;

    // `Clone` and `PartialEq` repeat what the presets already provide.
    #[derive(Clone, Key, PartialEq)] struct Tag(u8);
    #[cfg_attr(all(), derive(Key))] struct Mark(u8);
}

use orders::{Address, Order, OrderId, Status, UserId};

fn order(id: u64, status: Status, city: &str) -> Order {
    Order {
        id: OrderId(id),
        user: UserId(id * 10),
        status,
        ship_to: Address { city: city.to_owned(), ..Address::default() },
    }
}

fn main() {
    let orders = [order(1, Status::New, "Oslo"), order(2, Status::Paid, "Rome"), order(3, Status::Shipped, "Rome")];
    let (by_id, cities) = orders::index(&orders);
    assert_eq!(by_id, BTreeMap::from([(OrderId(1), UserId(10)), (OrderId(2), UserId(20)), (OrderId(3), UserId(30))]));
    assert_eq!(cities, HashSet::from([Address { city: "Rome".to_owned(), street: String::new() }]));
    assert_eq!(format!("{:?}", UserId(7)), "UserId(7)");
    assert!(Status::New < Status::Shipped);
    assert_eq!(orders[0].clone(), orders[0]);

    let tag = Tag(1);
    let copy = tag;
    assert_eq!(tag, copy);
    assert_eq!(HashSet::from([tag, copy, Tag(2)]).len(), 2);
    assert_eq!(format!("{:?}", Mark(3).clone()), "Mark(3)");
}
