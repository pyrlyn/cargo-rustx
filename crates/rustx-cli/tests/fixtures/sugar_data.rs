//! A user record with a constructor, getters, setters and a builder, written without sugar.

/// Someone who can sign in.
#[derive(Debug, Clone)]
pub struct User {
    id: u64,
    // Shown in the UI, never empty.
    name: String,
}

#[allow(dead_code)]
impl User {
    pub fn new(id: u64, name: String) -> Self {
        Self { id, name }
    }
    pub fn id(&self) -> u64 {
        self.id
    }
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn set_id(&mut self, id: u64) {
        self.id = id;
    }
    pub fn set_name(&mut self, name: String) {
        self.name = name;
    }
    pub fn builder() -> UserBuilder {
        UserBuilder {
            id: None,
            name: None,
        }
    }
}

pub struct UserBuilder {
    id: Option<u64>,
    name: Option<String>,
}

#[allow(dead_code)]
impl UserBuilder {
    pub fn id(mut self, id: u64) -> Self {
        self.id = Some(id);
        self
    }
    pub fn name(mut self, name: String) -> Self {
        self.name = Some(name);
        self
    }
    pub fn build(self) -> Result<User, &'static str> {
        Ok(User {
            id: self.id.ok_or("`User` builder is missing `id`")?,
            name: self.name.ok_or("`User` builder is missing `name`")?,
        })
    }
}

pub fn guest() -> User {
    User::builder().id(0).name(String::from("guest")).build().unwrap()
}
