//! A tree shared through `Rc<RefCell<..>>` and `Weak<RefCell<..>>`, written without sugar.

use std::cell::RefCell;
use std::rc::{Rc, Weak};

pub struct Node {
    pub size: u64,
    // Weak, so a tree does not keep itself alive.
    pub parent: Option<Weak<RefCell<Node>>>,
    pub children: Vec<Rc<RefCell<Node>>>,
}

pub type Link = Rc<RefCell<Node>>;

pub fn parent_of(node: &Rc<RefCell<Node>>) -> Option<Rc<RefCell<Node>>> {
    node.borrow().parent.as_ref().and_then(Weak::upgrade)
}

pub fn swap(node: Rc<RefCell<Node>>, with: Node) -> Node {
    // As sugar, `replace` would go through a borrow, so this parameter keeps its type.
    node.replace(with)
}

pub fn with_size(node: Link) -> (Rc<RefCell<Node>>, u64) {
    // A tuple element starts a group, where `rc` would read as a value.
    let size = node.borrow().size;
    (node, size)
}
