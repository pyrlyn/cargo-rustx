//! `Shared<T>`: the runtime type behind the `shared` sugar (`problems/12-arc-mutex.md`).

use std::fmt;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError, TryLockError};

/// A value shared between owners and threads: `Arc<Mutex<T>>` without the lock noise.
///
/// Cloning a `Shared` clones the handle, not the value: every clone sees the same `T`.
/// `shared s = e;` desugars to `Shared::new(e)`, and each field access on `s` takes the lock
/// for that one access. Use [`with`](Shared::with) or [`with_mut`](Shared::with_mut) to run a
/// group of operations under a single lock.
///
/// # Poisoning
///
/// A `std` mutex is poisoned when a thread panics while holding its lock. `Shared` does not
/// propagate that: [`lock`](Shared::lock), [`with`](Shared::with) and
/// [`with_mut`](Shared::with_mut) recover the guard with [`PoisonError::into_inner`] and
/// carry on. The panic itself already reaches its owner (through `JoinHandle::join`, or by
/// unwinding the current thread), so turning every later access into a second panic, as
/// `.lock().unwrap()` does, only cascades one failure across all threads; and the sugar makes
/// a lock look like plain field access, which has no failure path to report one through.
/// The cost is that a value a panicking writer left half-updated is visible afterwards; code
/// whose invariants span several fields can check [`is_poisoned`](Shared::is_poisoned).
///
/// # Examples
///
/// ```
/// use rustx::Shared;
///
/// let counter = Shared::new(0);
/// let handles: Vec<_> = (0..4)
///     .map(|_| {
///         let counter = counter.clone();
///         std::thread::spawn(move || *counter.lock() += 1)
///     })
///     .collect();
/// for h in handles {
///     h.join().unwrap();
/// }
/// assert_eq!(*counter.lock(), 4);
/// ```
pub struct Shared<T> {
    inner: Arc<Mutex<T>>,
}

impl<T> Shared<T> {
    /// Wraps `value` in a new shared handle.
    ///
    /// ```
    /// let names = rustx::Shared::new(vec!["a"]);
    /// assert_eq!(names.lock().len(), 1);
    /// ```
    #[must_use]
    pub fn new(value: T) -> Self {
        Self {
            inner: Arc::new(Mutex::new(value)),
        }
    }

    /// Locks the value and returns the guard; the lock is released when the guard drops.
    ///
    /// This is what field access on a `shared` binding desugars to: `s.count += 1` becomes
    /// `s.lock().count += 1`. A poisoned lock is recovered (see [`Shared`]).
    ///
    /// The lock is not reentrant: taking it again on the same thread while a guard is alive
    /// deadlocks or panics. A guard used as a temporary lives until the end of its statement,
    /// so two `lock()` calls in one statement, as in `(s.lock().a, s.lock().b)`, must instead
    /// share one guard or use [`with`](Shared::with).
    ///
    /// ```
    /// #[derive(Default)]
    /// struct State {
    ///     count: u32,
    /// }
    ///
    /// let s = rustx::Shared::new(State::default());
    /// s.lock().count += 1;
    /// assert_eq!(s.lock().count, 1);
    /// ```
    pub fn lock(&self) -> MutexGuard<'_, T> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Runs `f` on the value under one lock and returns its result.
    ///
    /// ```
    /// let pair = rustx::Shared::new((3, 4));
    /// let sum = pair.with(|(a, b)| a + b);
    /// assert_eq!(sum, 7);
    /// ```
    pub fn with<R>(&self, f: impl FnOnce(&T) -> R) -> R {
        f(&self.lock())
    }

    /// Runs `f` on the value mutably under one lock and returns its result, so no other
    /// owner observes the value between its operations.
    ///
    /// ```
    /// let stack = rustx::Shared::new(vec![1, 2]);
    /// let len = stack.with_mut(|v| {
    ///     v.push(3);
    ///     v.len()
    /// });
    /// assert_eq!(len, 3);
    /// ```
    pub fn with_mut<R>(&self, f: impl FnOnce(&mut T) -> R) -> R {
        f(&mut self.lock())
    }

    /// Whether a thread panicked while holding the lock, so the value may be half-updated.
    ///
    /// ```
    /// let s = rustx::Shared::new(0);
    /// let t = s.clone();
    /// let _ = std::thread::spawn(move || {
    ///     let _guard = t.lock();
    ///     panic!("poison the lock");
    /// })
    /// .join();
    /// assert!(s.is_poisoned());
    /// assert_eq!(*s.lock(), 0);
    /// ```
    #[must_use]
    pub fn is_poisoned(&self) -> bool {
        self.inner.is_poisoned()
    }
}

/// Clones the handle: both handles see the same value.
///
/// ```
/// let a = rustx::Shared::new(1);
/// let b = a.clone();
/// *b.lock() = 2;
/// assert_eq!(*a.lock(), 2);
/// ```
impl<T> Clone for Shared<T> {
    // Derived `Clone` would require `T: Clone`, but only the handle is cloned.
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

/// Shares `T::default()`.
///
/// ```
/// let s: rustx::Shared<Vec<u8>> = Default::default();
/// assert!(s.lock().is_empty());
/// ```
impl<T: Default> Default for Shared<T> {
    fn default() -> Self {
        Self::new(T::default())
    }
}

/// Same as [`Shared::new`].
///
/// ```
/// let s: rustx::Shared<&str> = "hi".into();
/// assert_eq!(*s.lock(), "hi");
/// ```
impl<T> From<T> for Shared<T> {
    fn from(value: T) -> Self {
        Self::new(value)
    }
}

/// Prints `Shared(value)`, or `Shared(<locked>)` while another guard holds the lock.
///
/// ```
/// let s = rustx::Shared::new(5);
/// assert_eq!(format!("{s:?}"), "Shared(5)");
/// let _guard = s.lock();
/// assert_eq!(format!("{s:?}"), "Shared(<locked>)");
/// ```
impl<T: fmt::Debug> fmt::Debug for Shared<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut out = f.debug_tuple("Shared");
        // `try_lock`, not `lock`: formatting a value whose guard this thread holds, such as
        // in a `dbg!` inside `with`, must not deadlock.
        match self.inner.try_lock() {
            Ok(guard) => out.field(&&*guard),
            Err(TryLockError::Poisoned(poisoned)) => out.field(&&*poisoned.into_inner()),
            Err(TryLockError::WouldBlock) => out.field(&format_args!("<locked>")),
        };
        out.finish()
    }
}

#[cfg(test)]
mod tests {
    use super::Shared;
    use std::thread;

    fn poison(s: &Shared<Vec<u32>>) {
        let t = s.clone();
        let joined = thread::spawn(move || {
            t.with_mut(|v| {
                v.push(1);
                panic!("writer dies mid-update");
            })
        })
        .join();
        assert!(joined.is_err());
    }

    #[test]
    fn clones_share_the_value() {
        let a = Shared::new(vec![1]);
        let b = a.clone();
        b.lock().push(2);
        assert_eq!(*a.lock(), [1, 2]);
    }

    #[test]
    fn clone_does_not_require_t_clone() {
        struct NotClone(u8);
        let a = Shared::new(NotClone(7));
        let b = a.clone();
        assert_eq!(b.lock().0, 7);
    }

    #[test]
    fn works_across_threads() {
        let s = Shared::new(0u32);
        let handles: Vec<_> = (0..8)
            .map(|_| {
                let s = s.clone();
                thread::spawn(move || {
                    for _ in 0..1000 {
                        *s.lock() += 1;
                    }
                })
            })
            .collect();
        for h in handles {
            h.join().unwrap();
        }
        assert_eq!(*s.lock(), 8000);
    }

    #[test]
    fn with_returns_the_closure_result() {
        let s = Shared::new(String::from("abc"));
        assert_eq!(s.with(String::len), 3);
    }

    #[test]
    fn with_mut_returns_the_closure_result_and_keeps_the_change() {
        let s = Shared::new(vec![1, 2]);
        assert_eq!(s.with_mut(Vec::pop), Some(2));
        assert_eq!(*s.lock(), [1]);
    }

    #[test]
    fn with_mut_groups_operations_under_one_lock() {
        let s = Shared::new((0u32, 0u32));
        let handles: Vec<_> = (0..4)
            .map(|_| {
                let s = s.clone();
                thread::spawn(move || {
                    for _ in 0..500 {
                        s.with_mut(|(a, b)| {
                            *a += 1;
                            *b += 1;
                        });
                        s.with(|(a, b)| assert_eq!(a, b));
                    }
                })
            })
            .collect();
        for h in handles {
            h.join().unwrap();
        }
        assert_eq!(*s.lock(), (2000, 2000));
    }

    #[test]
    fn poisoned_lock_is_recovered_with_the_partial_update() {
        let s = Shared::new(Vec::new());
        poison(&s);
        assert!(s.is_poisoned());
        assert_eq!(*s.lock(), [1]);
        assert_eq!(s.with(Vec::len), 1);
        s.with_mut(|v| v.push(2));
        assert_eq!(*s.lock(), [1, 2]);
    }

    #[test]
    fn healthy_lock_is_not_poisoned() {
        let s = Shared::new(Vec::new());
        s.with_mut(|v| v.push(1));
        assert!(!s.is_poisoned());
    }

    #[test]
    fn debug_shows_the_value_even_when_poisoned() {
        let s = Shared::new(Vec::new());
        assert_eq!(format!("{s:?}"), "Shared([])");
        poison(&s);
        assert_eq!(format!("{s:?}"), "Shared([1])");
    }

    #[test]
    fn debug_does_not_deadlock_while_this_thread_holds_the_lock() {
        let s = Shared::new(1);
        let shown = s.with(|_| format!("{s:?}"));
        assert_eq!(shown, "Shared(<locked>)");
    }

    #[test]
    fn default_and_from_wrap_the_value() {
        let d: Shared<u8> = Shared::default();
        let f = Shared::from(9u8);
        assert_eq!((*d.lock(), *f.lock()), (0, 9));
    }
}
