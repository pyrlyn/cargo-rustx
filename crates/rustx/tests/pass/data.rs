// Problem 08 in full, plus a generic struct and a `#[borrows]` struct.
rustx::sugar! {
    mod users {
        #[data(setters, with)]
        #[derive(Debug, Clone, PartialEq)]
        pub struct User {
            id: u64,
            name: String,
            email: String,
            #[default(true)]
            active: bool,
        }

        pub fn deactivate_by_domain(users: &mut [User], domain: &str) -> usize {
            var n = 0;
            for u in users.iter_mut() {
                if u.email().ends_with(domain) && u.active() {
                    u.set_active(false);
                    n += 1;
                }
            }
            n
        }
    }

    mod cache {
        use std::collections::HashMap;
        use std::hash::Hash;

        #[data]
        #[derive(Debug)]
        pub struct Cache<K: Hash + Eq, V> where V: Clone {
            map: HashMap<K, V>,
            #[default(16)]
            capacity: usize,
        }

        impl Cache<K, V> {
            pub fn get(&self, key: &K) -> Option<V> {
                self.map().get(key).cloned()
            }
        }
    }

    #[borrows]
    #[data(setters)]
    pub struct Line { text: &str, number: usize }

    fn local() -> u8 {
        #[data]
        struct Local { n: u8 }
        Local::new(7).n()
    }
}

use cache::Cache;
use users::User;

fn main() {
    let ann = User::new(1, "Ann".to_string(), "ann@corp.com".to_string());
    assert_eq!((ann.id(), ann.name(), ann.email(), ann.active()), (1, "Ann", "ann@corp.com", true));
    let bob = User::new(2, "Bob".to_string(), "bob@home.org".to_string())
        .with_email("bob@corp.com".to_string());
    let eve = User::new(3, "Eve".to_string(), "eve@home.org".to_string());
    let mut all = vec![ann, bob, eve];
    assert_eq!(users::deactivate_by_domain(&mut all, "@corp.com"), 2);
    let active: Vec<bool> = all.iter().map(User::active).collect();
    assert_eq!(active, [false, false, true]);
    all[0].set_name("Anna".to_string());
    assert_eq!(all[0].name(), "Anna");

    let cache = Cache::new([("a", 1)].into_iter().collect());
    assert_eq!((cache.capacity(), cache.get(&"a"), cache.get(&"b")), (16, Some(1), None));

    // The text borrows from the source, so it outlives the line.
    let src = String::from("first line");
    let text = {
        let mut line = Line::new("", 1);
        line.set_text(&src);
        assert_eq!(line.number(), 1);
        line.text()
    };
    assert_eq!(text, "first line");
    assert_eq!(Line::new(&src, 2).text(), "first line");

    assert_eq!(local(), 7);
}
