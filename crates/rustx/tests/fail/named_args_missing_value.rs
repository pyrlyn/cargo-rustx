rustx::sugar! {
    fn connect(host: &str, port: u16 = 5432) -> String {
        format!("{host}:{port}")
    }

    fn demo() -> String {
        connect("db", port:)
    }
}

fn main() {}
