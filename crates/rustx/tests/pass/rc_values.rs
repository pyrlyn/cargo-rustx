// `(rc name)` is the documented spelling for a bare name as a field value; it must not warn.
#![deny(unused_parens)]

rustx::sugar! {
    mod tree {
        use std::rc::Rc;

        pub struct Node {
            pub name: String,
            pub size: u64,
            pub parent: Option<weak Node>,
            pub children: Vec<rc Node>,
        }

        pub struct Forest {
            pub first: rc Node,
            pub rest: Vec<rc Node>,
            pub visits: rc usize,
        }

        // A struct literal as the value, at the tail of a body.
        pub fn new_node(name: &str, size: u64) -> rc Node {
            rc Node { name: name.to_string(), size, parent: None, children: Vec::new() }
        }

        pub fn leaf(name: &str) -> Node {
            Node { name: name.to_string(), size: 1, parent: None, children: Vec::new() }
        }

        pub fn add_child(parent: &rc Node, child: rc Node) {
            child.borrow_mut().parent = Some(Rc::downgrade(parent));
            parent.borrow_mut().size += child.borrow().size;
            parent.borrow_mut().children.push(child);
        }

        pub fn demo() -> (u64, usize, usize, usize) {
            let root = new_node("root", 0);
            // Arguments.
            add_child(&root, rc leaf("a"));
            add_child(&root, rc Node { size: 5, ..leaf("b") });
            // `let` values, with and without a written `rc T` type.
            let shared: rc Node = rc leaf("c");
            let counter = rc 0usize;
            let visits = 1;
            add_child(&root, shared.clone());
            *counter.borrow_mut() += root.borrow().children.len();
            // Struct field values; a bare name needs parentheses, or it reads as a typed field.
            let forest = Forest {
                first: rc leaf("d"),
                rest: vec![rc leaf("e"), shared],
                visits: (rc visits),
            };
            let total = root.borrow().size;
            (total, *counter.borrow(), forest.rest.len(), *forest.visits.borrow())
        }

        // Valid Rust that names things `rc` is left alone.
        pub fn strong(rc: &Rc<u8>) -> usize {
            let rc = rc.clone();
            Rc::strong_count(&rc)
        }
    }
}

fn main() {
    assert_eq!(tree::demo(), (7, 3, 2, 1));
}
