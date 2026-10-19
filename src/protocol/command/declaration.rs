#[derive(Clone)]
pub struct Declaration {
    pub name: String,
}

impl Declaration {
    pub fn new(name: String) -> Self {
        Self { name }
    }
}

// Question: should `Declaration` implement `Command` trait?
// The original code has implemented it, but I added some methods into `Command` trait, so I have to
// comment it out.
//
// impl Command for Declaration {
//     fn name(&self) -> &str {
//         &self.name
//     }
// }
