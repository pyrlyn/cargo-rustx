rustx::sugar! {
    fn connect(host: &str, port: u16) -> String {
        format!("{host}:{port}")
    }

    fn demo() -> String {
        connect("db", port: 6432)
    }
}

fn main() {}
