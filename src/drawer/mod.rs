mod okx;
mod text;

pub use okx::{AccountBalance, AssetValuation};

use serde::Deserialize;

#[derive(Deserialize)]
pub struct Layout {
    pub item: Vec<Drawer>,
}

#[derive(Clone, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Drawer {
    Okx(okx::Okx),
    #[allow(dead_code)]
    Text(text::Text),
}

#[cfg(test)]
mod tests {
    use super::{Drawer, Layout};

    #[test]
    fn parses_layout_items_into_drawers() {
        let layout: Layout = toml::from_str(
            r#"
                [[item]]
                type = "okx"
                apiKey = "api-key"
                secretKey = "secret-key"

                [[item]]
                type = "text"
                content = "hello"
            "#,
        )
        .expect("layout should parse");

        assert_eq!(layout.item.len(), 2);
        assert!(matches!(layout.item[0], Drawer::Okx(_)));
        assert!(matches!(layout.item[1], Drawer::Text(_)));
    }
}
