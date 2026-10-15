// M3 passes in one input: a `bound` preset used as an inline bound, an inline bound on an
// iterator parameter whose items a `from` lifetime ties to the result, and a `#[borrows]` type
// whose methods mix `from(Self)`, `into` and parameter defaults.
rustx::sugar! {
    bound Key = Clone + Eq + std::hash::Hash;

    pub struct Entry {
        pub name: String,
    }

    fn count_unique(items: Vec<K: Key>) -> usize {
        let set: std::collections::HashSet<K> = items.into_iter().collect();
        set.len()
    }

    fn numbered(it: I: Iterator<Item = &Entry>) -> iter (usize, &from(it) str) {
        it.enumerate().map(|(i, e)| (i + 1, e.name.as_str()))
    }

    fn longest(a: &str, b: &str) -> &from(a, b) str {
        if a.len() >= b.len() { a } else { b }
    }

    #[borrows]
    pub struct Doc {
        title: &str,
    }

    impl Doc {
        pub fn new(title: &from(Self) str) -> Self {
            Doc { title }
        }

        pub fn heading(&self, prefix: into String, level: usize = 1) -> String {
            format!("{} {prefix}{}", "#".repeat(level), self.title)
        }

        pub fn title(&self) -> &from(Self) str {
            self.title
        }
    }
}

fn main() {
    assert_eq!(count_unique(vec![1, 2, 2, 3]), 3);
    let entries = vec![Entry { name: "a".into() }, Entry { name: "bb".into() }];
    let out: Vec<(usize, &str)> = numbered(entries.iter()).collect();
    assert_eq!(out, vec![(1, "a"), (2, "bb")]);
    assert_eq!(longest("x", "yy"), "yy");

    let text = String::from("Intro");
    let doc = Doc::new(&text);
    assert_eq!(doc.heading("> ", None), "# > Intro");
    assert_eq!(doc.heading(String::from("* "), Some(2)), "## * Intro");
    // The title borrows from `text`, not from `doc`, so it outlives `doc`.
    let title = doc.title();
    drop(doc);
    assert_eq!(title, "Intro");
}
