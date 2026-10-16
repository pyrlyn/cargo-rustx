# `#[dyn]` traits with `async fn`

`#[dyn]` on a trait with `async fn` keeps it native and generates a `DynTrait` type that can be used as a trait object, through the `dynosaur` crate. Needs the [`dyn` feature](dyn-feature.md). Problem: [14, `async` with `Pin` and `Box` in traits](../../problems/14-async-traits.md).

```rust
rustx::sugar! {
    // dynosaur's expansion needs the trait at module level, not inside a fn body.
    mod store {
        #[dyn]
        pub trait Store {
            async fn get(&self, key: u32) -> Option<String>;
        }

        pub struct Memory;

        impl Store for Memory {
            async fn get(&self, key: u32) -> Option<String> {
                (key == 1).then(|| String::from("one"))
            }
        }

        pub async fn lookup(store: &DynStore<'_>, key: u32) -> Option<String> {
            store.get(key).await
        }
    }
}

use store::{DynStore, Memory, lookup};

fn block_on<F: std::future::Future>(future: F) -> F::Output {
    let mut future = std::pin::pin!(future);
    let mut cx = std::task::Context::from_waker(std::task::Waker::noop());
    loop {
        if let std::task::Poll::Ready(out) = future.as_mut().poll(&mut cx) {
            return out;
        }
    }
}

assert_eq!(block_on(lookup(DynStore::from_ref(&Memory), 1)), Some("one".to_string()));
let boxed: Box<DynStore<'_>> = DynStore::new_box(Memory);
assert_eq!(block_on(lookup(&boxed, 2)), None);
```

Becomes:

```rust
mod store {
    #[::rustx::dynosaur::dynosaur(pub DynStore = dyn(box)Store)]
    pub trait Store {
        async fn get(&self, key: u32) -> Option<String>;
    }
    pub struct Memory;
    impl Store for Memory {
        async fn get(&self, key: u32) -> Option<String> {
            (key == 1).then(|| String::from("one"))
        }
    }
    pub async fn lookup(store: &DynStore<'_>, key: u32) -> Option<String> {
        store.get(key).await
    }
}
```

`#[dyn(Name)]` picks the type's name; the default is `Dyn` plus the trait's name, with the trait's visibility. A trait without `#[dyn]` stays a native `async fn` trait.

## Limits

- The problem file sketches boxing wherever the trait is used as `dyn`; instead the trait is marked once, and the generated type is written where `dyn Trait` would be (`&DynStore<'_>`, `DynStore::from_ref(&x)`, `DynStore::new_box(x)`). `dyn Trait` and `~Trait` are not rewritten to it.
- Only inside `sugar!`: rustc rejects the keyword path `#[dyn]` before an attribute macro sees the item.
- The expansion names `::rustx::dynosaur`, so the `rustx` dependency must not be renamed. dynosaur's `bridge(..)` is not exposed.
- `#[dyn]` on anything but a trait, twice, or with a malformed argument is an error.
