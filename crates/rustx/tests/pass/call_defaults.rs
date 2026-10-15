rustx::sugar! {
    mod server {
        pub fn connect(host: &str, port: u16 = 5432, timeout_ms: u64 = 5_000) -> String {
            format!("{host}:{port} ({timeout_ms} ms)")
        }

        pub fn demo() -> Vec<String> {
            vec![
                connect("db"),
                connect("db", 6432),
                connect("db", 5432, 100),
            ]
        }

        pub struct Pool {
            size: usize,
        }

        impl Pool {
            pub fn new(size: usize = 4) -> Self {
                Pool { size }
            }

            pub fn spawn(&self, times: usize = 1) -> usize {
                self.size * times
            }

            pub fn doubled() -> Self {
                Self::new(scaled())
            }
        }

        fn scaled(factor: usize = 2, base: usize = Pool::new().spawn()) -> usize {
            factor * base
        }

        pub fn pools() -> [usize; 3] {
            [Pool::new().spawn(), Pool::new(2).spawn(3), Pool::doubled().spawn()]
        }
    }
}

fn main() {
    assert_eq!(
        server::demo(),
        ["db:5432 (5000 ms)", "db:6432 (5000 ms)", "db:5432 (100 ms)"]
    );
    assert_eq!(server::pools(), [4, 6, 8]);
    // Call sites outside `sugar!` see the real `Option` signature.
    assert_eq!(server::connect("db", Some(1), None), "db:1 (5000 ms)");
}
