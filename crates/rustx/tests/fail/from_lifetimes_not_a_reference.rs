rustx::sugar! {
    fn pick(n: usize, words: &[String]) -> &from(n) str {
        &words[n]
    }
}

fn main() {}
