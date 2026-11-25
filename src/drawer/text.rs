use serde::Deserialize;

#[derive(Clone, Deserialize)]
pub struct Text {
    content: String,
}

impl Text {
    pub fn content(&self) -> &str {
        &self.content
    }
}
