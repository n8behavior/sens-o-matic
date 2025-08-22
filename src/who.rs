#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Member {
    pub name: String,
    pub email: String,
}

impl Member {
    pub fn new(name: impl Into<String>, email: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            email: email.into(),
        }
    }
}
