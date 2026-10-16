// Problem 14's detailed example. `shared` is not lowered yet (T24), so the field is a plain
// `Mutex`, and the `dyn` parameters use the generated `DynStore` instead of `~Store`.
use std::future::Future;
use std::pin::pin;
use std::task::{Context, Poll, Waker};

rustx::sugar! {
mod store {
    use std::collections::HashMap;
    use std::sync::Mutex;

    #[dyn]
    pub trait Store: Send + Sync {
        async fn get(&self, key: &str) -> Option<Vec<u8>>;
        async fn put(&self, key: &str, value: Vec<u8>) -> Result<(), String>;
    }

    #[derive(Default)]
    pub struct MemStore {
        data: Mutex<HashMap<String, Vec<u8>>>,
    }

    impl Store for MemStore {
        async fn get(&self, key: &str) -> Option<Vec<u8>> {
            self.data.lock().unwrap().get(key).cloned()
        }

        async fn put(&self, key: &str, value: Vec<u8>) -> Result<(), String> {
            if value.len() > 1024 {
                return Err(format!("{}: value too large", key));
            }
            self.data.lock().unwrap().insert(key.to_string(), value);
            Ok(())
        }
    }

    pub async fn copy_key(src: &DynStore<'_>, dst: &DynStore<'_>, key: &str) -> Result<bool, String> {
        match src.get(key).await {
            Some(v) => dst.put(key, v).await.map(|_| true),
            None => Ok(false),
        }
    }
}
}

rustx::sugar! {
    // A given name, a private trait and a trait that stays native next to it.
    #[dyn(AnyCounter)]
    trait Counter {
        async fn next(&mut self) -> u32;
    }

    trait Native {
        async fn native(&self) -> u32;
    }

    struct Steps(u32);

    impl Counter for Steps {
        async fn next(&mut self) -> u32 {
            self.0 += 1;
            self.0
        }
    }

    impl Native for Steps {
        async fn native(&self) -> u32 {
            self.0
        }
    }

    async fn twice(counter: &mut AnyCounter<'_>) -> u32 {
        counter.next().await + counter.next().await
    }
}

use store::{DynStore, MemStore, Store};

/// Polls `future` to completion; every future here is ready on its first poll.
fn block_on<F: Future>(future: F) -> F::Output {
    let mut future = pin!(future);
    let mut cx = Context::from_waker(Waker::noop());
    loop {
        if let Poll::Ready(out) = future.as_mut().poll(&mut cx) {
            return out;
        }
    }
}

fn main() {
    let (src, dst) = (MemStore::default(), MemStore::default());
    block_on(src.put("a", vec![1, 2])).unwrap();

    let boxed: Box<DynStore<'_>> = DynStore::new_box(MemStore::default());
    assert_eq!(block_on(store::copy_key(DynStore::from_ref(&src), DynStore::from_ref(&dst), "a")), Ok(true));
    assert_eq!(block_on(store::copy_key(&boxed, DynStore::from_ref(&dst), "missing")), Ok(false));
    assert_eq!(block_on(dst.get("a")), Some(vec![1, 2]));
    assert_eq!(
        block_on(src.put("big", vec![0; 2048])),
        Err("big: value too large".to_string())
    );

    let mut steps = Steps(0);
    assert_eq!(block_on(twice(AnyCounter::from_mut(&mut steps))), 3);
    assert_eq!(block_on(steps.native()), 2);
}
