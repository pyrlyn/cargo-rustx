//! Conditional methods must disappear from both implementations.
use boilerless::bind;

trait Service {
    fn active(&self) -> u8;
    #[cfg(any())]
    fn disabled(&self) -> u8;
}

struct Server;

#[bind(Service)]
impl Server {
    fn active(&self) -> u8 { 7 }
    #[cfg(any())]
    fn disabled(&self) -> u8 { 9 }
    #[cfg_attr(all(), cfg(any()))]
    fn absent(&self) {}
}

fn main() {
    assert_eq!(Service::active(&Server), 7);
}
