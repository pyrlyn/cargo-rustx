//! A counter written without sugar.

/// Counts the words of `text` that are longer than `min`.
pub fn long_words(text: &str, min: usize) -> usize {
    let mut count = 0; // only words longer than `min`
    for word in text.split_whitespace() {
        // `let` stays: `len` is never reassigned.
        let len = word.chars().count();
        if len > min {
            count += 1;
        }
    }
    count
}

pub fn first_long(text: &str, min: usize) -> Option<String> {
    let mut found: Option<String> = None;
    for word in text.split_whitespace().rev() {
        if word.len() > min {
            found = Some(word.to_owned());
        }
    }
    found
}
