// The AST pass cannot see into a macro call, so it could not rewrite the binding's scope.
rustx::sugar! {
    fn f() -> Vec<u8> {
        vec![{
            shared n = 1;
            n.lock().clone()
        }]
    }
}

fn main() {}
