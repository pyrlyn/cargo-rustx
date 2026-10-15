//! `with` / `with_mut` for `rc T` values (`problems/16-rc-refcell.md`).

use std::cell::RefCell;
use std::rc::Rc;

/// `with` and `with_mut` on `Rc<RefCell<T>>`, the type `rc T` expands to.
///
/// Field access on an `rc` binding borrows once per statement. These run a group of operations
/// under one borrow, and `with_mut` reaches methods that take `&mut self`, which the sugar cannot
/// tell from `&self` ones and so calls through `.borrow()`.
///
/// The sugar never calls these methods itself, so code that does brings the trait into scope:
/// `use rustx::RcExt;`.
///
/// ```
/// use std::cell::RefCell;
/// use std::rc::Rc;
///
/// use rustx::RcExt;
///
/// let stack = Rc::new(RefCell::new(vec![1, 2]));
/// let len = stack.with_mut(|v| {
///     v.push(3);
///     v.len()
/// });
/// assert_eq!(len, 3);
/// assert_eq!(stack.with(|v| v[0]), 1);
/// ```
pub trait RcExt<T: ?Sized> {
    /// Runs `f` on the value under one shared borrow and returns its result.
    ///
    /// # Panics
    ///
    /// If the value is mutably borrowed, as [`RefCell::borrow`] does.
    fn with<R>(&self, f: impl FnOnce(&T) -> R) -> R;

    /// Runs `f` on the value under one mutable borrow and returns its result.
    ///
    /// # Panics
    ///
    /// If the value is borrowed, as [`RefCell::borrow_mut`] does.
    fn with_mut<R>(&self, f: impl FnOnce(&mut T) -> R) -> R;
}

impl<T: ?Sized> RcExt<T> for Rc<RefCell<T>> {
    fn with<R>(&self, f: impl FnOnce(&T) -> R) -> R {
        f(&self.borrow())
    }

    fn with_mut<R>(&self, f: impl FnOnce(&mut T) -> R) -> R {
        f(&mut self.borrow_mut())
    }
}

#[cfg(test)]
mod tests {
    use super::RcExt;
    use std::cell::RefCell;
    use std::rc::Rc;

    #[test]
    fn with_mut_keeps_the_change_for_every_clone() {
        let a = Rc::new(RefCell::new(vec![1]));
        let b = Rc::clone(&a);
        assert_eq!(b.with_mut(Vec::pop), Some(1));
        assert!(a.with(Vec::is_empty));
    }

    #[test]
    fn reads_nest_inside_with() {
        let n = Rc::new(RefCell::new(2));
        assert_eq!(n.with(|a| n.with(|b| a * b)), 4);
    }

    #[test]
    #[should_panic(expected = "already borrowed")]
    fn with_mut_while_borrowed_panics_like_refcell() {
        let n = Rc::new(RefCell::new(0));
        n.with(|_| n.with_mut(|v| *v += 1));
    }

    #[test]
    fn works_on_unsized_values() {
        let s: Rc<RefCell<[u8]>> = Rc::new(RefCell::new([1, 2, 3]));
        s.with_mut(<[u8]>::reverse);
        assert_eq!(s.with(|v| v[0]), 3);
    }
}
