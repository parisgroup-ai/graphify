use crate::Service;

pub struct Handler {
    name: String,
}

impl Handler {
    pub fn new() -> Self {
        Self {
            name: String::from("default"),
        }
    }

    pub fn handle(&self) {
        println!("handling: {}", self.name);
        validate(&self.name);
    }
}

fn validate(name: &str) -> bool {
    !name.is_empty()
}
