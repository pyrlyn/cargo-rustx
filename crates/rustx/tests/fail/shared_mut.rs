rustx::sugar! {
    fn f() {
        shared mut count = 0;
        count.lock();
    }
}

fn main() {}
