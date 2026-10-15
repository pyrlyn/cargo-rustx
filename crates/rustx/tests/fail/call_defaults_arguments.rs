rustx::sugar! {
    fn connect(host: &str, port: u16 = 5432) -> String {
        format!("{host}:{port}")
    }

    fn demo() -> Vec<String> {
        vec![connect("db", 1, 2), connect()]
    }
}

fn main() {}
