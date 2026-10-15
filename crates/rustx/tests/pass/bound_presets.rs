// The problem 21 example with plain `<K: Key>` generics: inline bounds (`&[K: Key]`) are a
// separate sugar item.
rustx::sugar! {
    mod generic {
        use std::collections::{HashMap, HashSet};
        use std::fmt::Debug;
        use std::hash::Hash;
        use std::ops::{Add, Mul};

        pub bound Key = Clone + Debug + Eq + Hash + Send + Sync + 'static;
        pub bound Num = Copy + Default + Add<Output = Self> + Mul<Output = Self>;

        pub fn dedup_count<K: Key>(items: &[K]) -> HashMap<K, usize> {
            var out = HashMap::new();
            for k in items {
                *out.entry(k.clone()).or_insert(0) += 1;
            }
            out
        }

        pub fn first_dup<K: Key>(items: &[K]) -> Option<K> {
            var seen = HashSet::new();
            items.iter().find(|k| !seen.insert(*k)).cloned()
        }

        pub fn dot<T: Num>(a: &[T], b: &[T]) -> T {
            a.iter().zip(b).fold(T::default(), |acc, (&x, &y)| acc + x * y)
        }

        pub fn norm_sq<T: Num>(a: &[T]) -> T {
            dot(a, a)
        }
    }
}

// A preset is a real trait, so code outside `sugar!` can require it too.
fn require_key<K: generic::Key>() {}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct UserId(u32);

fn main() {
    let words = ["a", "b", "a", "c", "a"];
    let counts = generic::dedup_count(&words);
    assert_eq!(counts["a"], 3);
    assert_eq!(counts["c"], 1);
    assert_eq!(generic::first_dup(&words), Some("a"));

    // Any type with the listed bounds satisfies the preset through the blanket impl.
    let ids = [UserId(1), UserId(2), UserId(2)];
    assert_eq!(generic::first_dup(&ids), Some(UserId(2)));
    require_key::<UserId>();
    require_key::<String>();

    assert_eq!(generic::dot(&[1, 2, 3], &[4, 5, 6]), 32);
    assert_eq!(generic::norm_sq(&[3.0, 4.0]), 25.0);
}
