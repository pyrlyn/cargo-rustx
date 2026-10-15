rustx::sugar! {
    mod server {
        pub fn connect(host: &str, port: u16 = 5432, timeout_ms: u64 = 5_000) -> String {
            format!("{host}:{port} ({timeout_ms} ms)")
        }

        pub fn label(host: &str, name: String = host.to_uppercase(), mut repeat: usize = 1) -> String {
            let mut out = String::new();
            while repeat > 0 {
                out.push_str(&name);
                repeat -= 1;
            }
            out
        }

        pub struct Pool;

        impl Pool {
            pub fn size(&self, (min, max): (usize, usize) = (1, 4)) -> usize {
                max - min
            }
        }
    }
}

fn main() {
    // Call sites outside `sugar!` see the real `Option` signature.
    assert_eq!(server::connect("db", None, None), "db:5432 (5000 ms)");
    assert_eq!(server::connect("db", Some(6432), None), "db:6432 (5000 ms)");
    assert_eq!(server::connect("db", None, Some(100)), "db:5432 (100 ms)");
    assert_eq!(server::label("db", None, None), "DB");
    assert_eq!(server::label("db", Some("x".to_owned()), Some(3)), "xxx");
    assert_eq!(server::Pool.size(None), 3);
    assert_eq!(server::Pool.size(Some((2, 10))), 8);
}
