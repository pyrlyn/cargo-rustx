//! Contacts built from anything that converts into their fields, written without sugar.

pub struct Contact {
    name: String,
    tags: Vec<String>,
}

impl Contact {
    pub fn new(name: impl Into<String>) -> Self {
        // A comment next to the conversion stays where it is.
        let name: String = Into::into(name);
        Self { name, tags: Vec::new() }
    }

    pub fn tag(&mut self, tag: impl Into<String>, more: impl Into<Vec<String>>) {
        let mut more: Vec<String> = Into::into(more);
        more.push(Into::into(tag));
        self.tags.append(&mut more);
    }

    // `.into()` is not what the sugar writes, so this parameter stays as written.
    pub fn rename(&mut self, name: impl Into<String>) {
        self.name = name.into();
    }
}
