//! A shared counter list that borrows explicitly, written without sugar.

use std::cell::RefCell;
use std::rc::Rc;

pub struct Counters {
    pub total: u64,
    pub items: Vec<u64>,
}

pub fn record(counters: &Rc<RefCell<Counters>>, value: u64) -> usize {
    counters.borrow_mut().total += value;
    // Each access borrows on its own statement, as the sugar does.
    counters.borrow_mut().items.push(value);
    println!("total {}", counters.borrow().total);
    counters.borrow().items.len()
}

pub fn fresh() -> u64 {
    let seen = Rc::new(RefCell::new(Vec::new()));
    seen.borrow_mut().push(1);
    seen.borrow().len() as u64
}

pub fn held(counters: Rc<RefCell<Counters>>) -> u64 {
    // A held guard is a borrow the sugar does not write, so every borrow here stays.
    let guard = counters.borrow();
    guard.total + counters.borrow().total
}
