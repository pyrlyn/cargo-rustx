rustx::sugar! {
    mod file {
        pub fn open(path: &str, mode: u8 = 0) {}
    }

    mod socket {
        pub fn open(path: &str) {}
    }

    fn demo() {
        file::open("a");
        socket::open("b");
    }
}

fn main() {}
