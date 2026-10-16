// Problem 16's detailed example, plus the forms the problem's prose describes.
rustx::sugar! {
    mod tree {
        use std::rc::Rc;

        use rustx::RcExt;

        pub struct Node {
            pub name: String,
            pub size: u64,
            pub parent: Option<weak Node>,
            pub children: Vec<rc Node>,
        }

        impl Node {
            pub fn grow(&mut self, by: u64) {
                self.size += by;
            }
        }

        pub fn new_node(name: &str, size: u64) -> rc Node {
            rc Node { name: name.to_string(), size, parent: None, children: Vec::new() }
        }

        pub fn add_child(parent: &rc Node, child: rc Node) {
            child.parent = Some(Rc::downgrade(parent));
            parent.size += child.size;
            parent.children.push(child);
        }

        pub fn path(node: &rc Node) -> String {
            var parts = vec![node.name.clone()];
            var cur = node.parent.as_ref().and_then(|w| w.upgrade());
            while let Some(p) = cur {
                // `p` has no written type, so its borrows stay explicit
                parts.push(p.borrow().name.clone());
                cur = p.borrow().parent.as_ref().and_then(|w| w.upgrade());
            }
            parts.reverse();
            parts.join("/")
        }

        pub fn count(node: &rc Node) -> usize {
            1 + node.children.iter().map(count).sum<usize>()
        }

        pub fn demo() -> (String, usize, u64) {
            let root = new_node("root", 0);
            let src = new_node("src", 0);
            let main = new_node("main.rs", 120);
            add_child(&src, main.clone());
            add_child(&root, src);
            let total = root.borrow().size;   // `root` has no written type either
            (path(&main), count(&root), total)
        }

        // A read and a write of the same value in one statement must not panic.
        pub fn double(node: &rc Node) -> u64 {
            node.size = node.size * 2 + node.children.len() as u64;
            node.size
        }

        // A user `&mut self` method goes through `with_mut`.
        pub fn grow_all(nodes: Vec<rc Node>, by: u64) -> u64 {
            let sum: rc u64 = Rc::new(std::cell::RefCell::new(0));
            for n in &nodes {
                n.with_mut(|n| n.grow(by));
            }
            nodes.iter().for_each(|n: &rc Node| *sum.borrow_mut() += n.size);
            *sum.borrow()
        }

        pub fn names() -> Vec<String> {
            let seen = rc Vec::new();
            seen.push("a".to_string());
            seen.extend(["b".to_string()]);
            seen.with(|v| v.clone())
        }

        // A hand-written `Rc<RefCell<T>>` is plain Rust: `replace` stays `RefCell::replace`.
        pub fn reset(n: &::std::rc::Rc<::core::cell::RefCell<u64>>) -> u64 {
            n.replace(0)
        }
    }
}

fn main() {
    assert_eq!(tree::demo(), ("root/src/main.rs".to_string(), 3, 120));
    let n = tree::new_node("n", 5);
    tree::add_child(&n, tree::new_node("c", 1));
    assert_eq!(tree::double(&n), 13);
    assert_eq!(tree::grow_all(vec![n.clone(), tree::new_node("m", 2)], 1), 17);
    assert_eq!(tree::names(), ["a", "b"]);
    let v = std::rc::Rc::new(std::cell::RefCell::new(7));
    assert_eq!(tree::reset(&v), 7);
    assert_eq!(*v.borrow(), 0);
}
