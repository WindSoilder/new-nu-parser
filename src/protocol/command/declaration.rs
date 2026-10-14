use super::Command;

#[derive(Clone)]
pub struct Declaration {
    name: String,
}

impl Declaration {
    pub fn new(name: String) -> Self {
        Self { name }
    }
}

impl Command for Declaration {
    fn name(&self) -> &str {
        &self.name
    }
}

