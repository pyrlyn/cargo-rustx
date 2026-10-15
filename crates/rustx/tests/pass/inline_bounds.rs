rustx::sugar! {
    mod cache {
        use std::collections::HashMap;
        use std::fmt::Debug;
        use std::hash::Hash;

        pub struct Cache<K: Eq + Hash + Clone + Debug, V: Clone> {
            pub map: HashMap<K, V>,
            pub hits: u64,
        }

        impl Cache<K, V> {
            pub fn new() -> Self {
                Cache { map: HashMap::new(), hits: 0 }
            }

            pub fn get_or_insert_with(
                &mut self,
                key: K,
                make: F: FnOnce(&K) -> Result<V, E>,
            ) -> Result<V, E: Debug> {
                if let Some(v) = self.map.get(&key) {
                    self.hits += 1;
                    return Ok(v.clone());
                }
                let v = make(&key)?;
                self.map.insert(key, v.clone());
                Ok(v)
            }
        }

        pub fn warm_up(cache: &mut Cache<K, V>, items: I: IntoIterator<Item = (K, V)>) {
            for (k, v) in items {
                cache.map.insert(k, v);
            }
        }
    }

    mod run {
        use std::error::Error;
        use std::fmt::Debug;

        pub fn run(x: T: Clone + Debug + Send) -> Result<T, E: Error> {
            Ok(x.clone())
        }

        pub trait Describe {
            fn describe(&self, prefix: P: AsRef<str>) -> String;
        }

        impl Describe for u8 {
            fn describe(&self, prefix: P: AsRef<str>) -> String {
                format!("{}{self}", prefix.as_ref())
            }
        }
    }
}

use cache::Cache;

#[derive(Debug)]
struct Missing;

impl std::fmt::Display for Missing {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("missing")
    }
}

impl std::error::Error for Missing {}

fn main() {
    let mut cache: Cache<&str, u32> = Cache::new();
    cache::warm_up(&mut cache, [("a", 1), ("b", 2)]);
    assert_eq!(cache.get_or_insert_with("a", |_| Err::<u32, Missing>(Missing)).ok(), Some(1));
    assert_eq!(cache.hits, 1);
    assert_eq!(cache.get_or_insert_with("c", |k| Ok::<u32, Missing>(k.len() as u32 * 7)).ok(), Some(7));
    assert!(cache.get_or_insert_with("d", |_| Err::<u32, Missing>(Missing)).is_err());
    assert_eq!(cache.map.len(), 3);

    let ran: Result<Vec<u8>, Missing> = run::run(vec![1, 2]);
    assert_eq!(ran.ok(), Some(vec![1, 2]));

    use run::Describe;
    assert_eq!(7u8.describe("n="), "n=7");
    assert_eq!(7u8.describe(String::from("#")), "#7");
}
