// The problem's example, with the named-argument calls of `demo` written as in the problem.
rustx::sugar! {
    mod server {
        pub fn connect(host: &str, port: u16 = 5432, timeout_ms: u64 = 5_000) -> String {
            format!("{host}:{port} ({timeout_ms} ms)")
        }

        pub fn demo() -> Vec<String> {
            vec![
                connect("db"),
                connect("db", port: 6432),
                connect("db", timeout_ms: 100),
            ]
        }

        pub struct Pool {
            size: usize,
        }

        impl Pool {
            pub fn new(size: usize = 4) -> Self {
                Pool { size }
            }

            pub fn spawn(&self, times: usize = 1, extra: usize = 0) -> usize {
                self.size * times + extra
            }
        }

        pub fn pools() -> [usize; 3] {
            let add = |a: usize, b: usize| a + b;
            [
                Pool::new(size: 2).spawn(extra: 1),
                Pool::new().spawn(extra: add(1, 2), times: 2),
                Pool::new(size: 1).spawn(3, extra: 4),
            ]
        }
    }
}

fn main() {
    assert_eq!(
        server::demo(),
        ["db:5432 (5000 ms)", "db:6432 (5000 ms)", "db:5432 (100 ms)"]
    );
    assert_eq!(server::pools(), [3, 11, 7]);
}
