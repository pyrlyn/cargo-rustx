rustx::sugar! {
    fn connect(host: &str, port: u16 = 5432, timeout_ms: u64 = 5_000) -> String {
        format!("{host}:{port} ({timeout_ms} ms)")
    }

    fn demo() -> Vec<String> {
        vec![
            connect("db", prot: 1),
            connect("db", 1, port: 2),
            connect("db", port: 1, 2),
            connect(host: "db"),
        ]
    }
}

fn main() {}
