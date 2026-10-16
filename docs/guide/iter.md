# `iter T` return types

`-> iter T` is `-> impl Iterator<Item = T> + '_`. Problem: [09, `impl Trait` and iterator returns](../../problems/09-impl-iterator.md).

```rust
rustx::sugar! {
    struct Inventory {
        items: Vec<String>,
    }

    impl Inventory {
        fn long_names(&self) -> iter &String {
            self.items.iter().filter(|s| s.len() > 3)
        }
    }

    fn evens(xs: &[u32]) -> iter &from(xs) u32 {
        xs.iter().filter(|x| *x % 2 == 0)
    }
}

let inv = Inventory { items: vec!["axe".into(), "rope".into()] };
assert_eq!(inv.long_names().collect::<Vec<_>>(), ["rope"]);
assert_eq!(evens(&[1, 2, 4]).copied().collect::<Vec<_>>(), [2, 4]);
```

Becomes:

```rust
struct Inventory {
    items: Vec<String>,
}
impl Inventory {
    fn long_names(&self) -> impl ::core::iter::Iterator<Item = &String> + '_ {
        self.items.iter().filter(|s| s.len() > 3)
    }
}
fn evens<'__rustx_from_xs>(
    xs: &'__rustx_from_xs [u32],
) -> impl ::core::iter::Iterator<Item = &'__rustx_from_xs u32> + '__rustx_from_xs {
    xs.iter().filter(|x| *x % 2 == 0)
}
```

With [`&from(x)`](from-lifetimes.md) in the item type, the `from` lifetime replaces `'_`.

## Limits

- On edition 2024 `-> impl Trait` already captures every lifetime, so `iter T` mainly saves typing there; the expansion keeps `+ '_` so it works on both editions.
- `iter` is an ordinary identifier, so it is sugar only in a recognisable type position (after `->`, `:`, `<`, `&`, ...). At the start of a group or after `,` outside `<..>`, write the plain form.
