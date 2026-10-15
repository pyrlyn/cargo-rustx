rustx::sugar! {
    mod server {
        #[derive(Debug, Clone, PartialEq)]
        pub struct ServerConfig {
            pub host: String = "127.0.0.1",
            pub port: u16 = 8080,
            pub workers: usize = 4,
            pub timeout_ms: u64 = 30_000,
            pub tls: bool = false,
        }

        pub fn dev() -> ServerConfig {
            ServerConfig { port: 3000, .. }
        }

        pub fn prod(host: &str) -> ServerConfig {
            ServerConfig { host: host.to_string(), workers: 16, tls: true, .. }
        }

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

        #[derive(Debug, PartialEq)]
        pub struct Pool<T: Clone> {
            pub items: Vec<T>,
            pub size: usize = 2 * 4,
        }

        // Struct patterns keep `..` as the rest of the pattern.
        pub fn describe(config: &ServerConfig) -> String {
            let ServerConfig { host, .. } = config;
            let port;
            ServerConfig { port, .. } = config.clone();
            match config {
                ServerConfig { tls: true, .. } => format!("tls {host}:{port}"),
                _ if matches!(config, ServerConfig { workers: 4, .. }) => format!("{host}:{port}"),
                _ => String::new(),
            }
        }

        pub fn presets() -> Vec<ServerConfig> {
            vec![ServerConfig { .. }, ServerConfig { tls: true, .. }]
        }
    }
}

fn main() {
    use server::{Pool, ServerConfig};

    let dev = server::dev();
    assert_eq!(dev.host, "127.0.0.1");
    assert_eq!((dev.port, dev.workers, dev.timeout_ms, dev.tls), (3000, 4, 30_000, false));

    let prod = server::prod("example.com");
    assert_eq!(prod.host, "example.com");
    assert_eq!((prod.port, prod.workers, prod.timeout_ms, prod.tls), (8080, 16, 30_000, true));

    assert_eq!(ServerConfig::default().port, 8080);
    assert_eq!(
        server::demo(),
        ["db:5432 (5000 ms)", "db:6432 (5000 ms)", "db:5432 (100 ms)"]
    );
    assert_eq!(Pool::<u8>::default(), Pool { items: Vec::new(), size: 8 });
    assert_eq!(server::describe(&dev), "127.0.0.1:3000");
    assert_eq!(server::describe(&prod), "tls example.com:8080");
    assert_eq!(server::presets()[0], ServerConfig::default());
    assert!(server::presets()[1].tls);
}
