//! Counters shared through `Rc<RefCell<..>>`, written without sugar.

use std::cell::RefCell;
use std::rc::Rc;

pub fn handles() -> Vec<Rc<RefCell<u32>>> {
    // One value behind every handle.
    let counter = Rc::new(RefCell::new(0));
    *counter.borrow_mut() += 1;
    let mut all = Vec::new();
    all.push(counter.clone());
    all.push(Rc::new(RefCell::new(10)));
    all
}

pub fn reset() -> u32 {
    let counter = Rc::new(RefCell::new(5));
    // As sugar, `replace` would go through a borrow, so this binding stays as written.
    counter.replace(0)
}
