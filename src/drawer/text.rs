use serde::Deserialize;

#[derive(Clone, Deserialize)]
pub struct Text {
    #[allow(dead_code)]
    content: String,
}
