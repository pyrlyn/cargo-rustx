rustx::sugar! {
    mod tree {
        use std::cell::RefCell;
        use std::rc::Rc;

        pub struct Node {
            pub name: String,
            pub size: u64,
            pub parent: Option<weak Node>,
            pub children: Vec<rc Node>,
        }

        pub fn new_node(name: &str, size: u64) -> rc Node {
            Rc::new(RefCell::new(Node {
                name: name.to_string(),
                size,
                parent: None,
                children: Vec::new(),
            }))
        }

        pub fn add_child(parent: &rc Node, child: rc Node) {
            child.borrow_mut().parent = Some(Rc::downgrade(parent));
            parent.borrow_mut().size += child.borrow().size;
            parent.borrow_mut().children.push(child);
        }

        pub fn path(node: &rc Node) -> String {
            let mut parts = vec![node.borrow().name.clone()];
            let mut cur = node.borrow().parent.as_ref().and_then(|w| w.upgrade());
            while let Some(p) = cur {
                parts.push(p.borrow().name.clone());
                cur = p.borrow().parent.as_ref().and_then(|w| w.upgrade());
            }
            parts.reverse();
            parts.join("/")
        }

        pub fn count(node: &rc Node) -> usize {
            1 + node.borrow().children.iter().map(count).sum::<usize>()
        }

        // Valid Rust that uses the prefix names as identifiers is left alone.
        pub fn strong(rc: &Rc<u8>) -> usize {
            let weak = Rc::downgrade(rc);
            let rc = rc.clone();
            Rc::strong_count(&rc) + weak.weak_count()
        }

        pub fn demo() -> (String, usize, u64) {
            let root = new_node("root", 0);
            let src = new_node("src", 0);
            let main = new_node("main.rs", 120);
            add_child(&src, main.clone());
            add_child(&root, src);
            let total = root.borrow().size;
            (path(&main), count(&root), total)
        }
    }
}

fn main() {
    assert_eq!(tree::demo(), ("root/src/main.rs".to_string(), 3, 120));
    assert_eq!(tree::strong(&std::rc::Rc::new(7)), 3);
}
