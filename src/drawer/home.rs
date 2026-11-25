use serde::Deserialize;
use std::collections::HashMap;
use std::time::Duration;

const PAGE_ID: &str = "42327bdc5727800a9d7cd527d675c152";
const API_BASE_URL: &str = "https://api.notion.com/v1";
const API_VERSION: &str = "2022-06-28";

#[derive(Clone, Deserialize)]
pub struct Home {
    notion: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct HomeContent {
    pub title: String,
    pub lines: Vec<String>,
}

#[derive(Deserialize)]
struct PageResponse {
    #[serde(default)]
    properties: HashMap<String, PageProperty>,
}

#[derive(Deserialize, Default)]
struct PageProperty {
    #[serde(default)]
    title: Vec<RichText>,
}

#[derive(Deserialize)]
struct BlockChildren {
    #[serde(default)]
    results: Vec<Block>,
    #[serde(default)]
    has_more: bool,
    next_cursor: Option<String>,
}

#[derive(Deserialize)]
struct Block {
    #[serde(rename = "type")]
    kind: String,
    paragraph: Option<BlockContent>,
    heading_1: Option<BlockContent>,
    heading_2: Option<BlockContent>,
    heading_3: Option<BlockContent>,
    bulleted_list_item: Option<BlockContent>,
    numbered_list_item: Option<BlockContent>,
    to_do: Option<BlockContent>,
    quote: Option<BlockContent>,
    code: Option<BlockContent>,
    callout: Option<BlockContent>,
}

#[derive(Deserialize, Default)]
struct BlockContent {
    #[serde(default)]
    rich_text: Vec<RichText>,
    #[serde(default)]
    checked: bool,
}

#[derive(Deserialize)]
struct RichText {
    #[serde(default)]
    plain_text: String,
}

impl Home {
    pub async fn fetch(self) -> Result<HomeContent, String> {
        if self.notion.trim().is_empty() {
            return Err("Notion API key is empty; set `notion` in conf/layout.toml.".to_owned());
        }

        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(15))
            .build()
            .map_err(|error| format!("could not create Notion HTTP client: {error}"))?;

        let page: PageResponse = request_json(
            &client,
            &self.notion,
            format!("{API_BASE_URL}/pages/{PAGE_ID}"),
        )
        .await?;
        let title = page
            .properties
            .values()
            .find(|property| !property.title.is_empty())
            .map(|property| plain_text(&property.title))
            .filter(|title| !title.is_empty())
            .unwrap_or_else(|| "Home".to_owned());

        let mut lines = Vec::new();
        let mut cursor: Option<String> = None;
        loop {
            let mut request = client.get(format!("{API_BASE_URL}/blocks/{PAGE_ID}/children"));
            if let Some(cursor) = &cursor {
                request = request.query(&[("start_cursor", cursor)]);
            }
            let response = request
                .bearer_auth(&self.notion)
                .header("Notion-Version", API_VERSION)
                .send()
                .await
                .map_err(|error| format!("Notion page content request failed: {error}"))?;
            let children: BlockChildren = decode_response(response, "page content").await?;
            lines.extend(children.results.iter().filter_map(Block::display_line));

            if !children.has_more {
                break;
            }
            cursor = children.next_cursor;
            if cursor.is_none() {
                return Err(
                    "Notion returned more page content without a pagination cursor.".to_owned(),
                );
            }
        }

        Ok(HomeContent { title, lines })
    }
}

async fn request_json<T: for<'de> Deserialize<'de>>(
    client: &reqwest::Client,
    api_key: &str,
    url: String,
) -> Result<T, String> {
    let response = client
        .get(url)
        .bearer_auth(api_key)
        .header("Notion-Version", API_VERSION)
        .send()
        .await
        .map_err(|error| format!("Notion page request failed: {error}"))?;
    decode_response(response, "page").await
}

async fn decode_response<T: for<'de> Deserialize<'de>>(
    response: reqwest::Response,
    resource: &str,
) -> Result<T, String> {
    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|error| format!("could not read Notion {resource} response: {error}"))?;

    if !status.is_success() {
        let message = serde_json::from_str::<NotionError>(&body)
            .map(|error| error.message)
            .unwrap_or(body);
        return Err(format!(
            "Notion {resource} request returned {status}: {message}"
        ));
    }

    serde_json::from_str(&body)
        .map_err(|error| format!("could not parse Notion {resource} response: {error}"))
}

#[derive(Deserialize)]
struct NotionError {
    message: String,
}

impl Block {
    fn display_line(&self) -> Option<String> {
        let (content, prefix) = match self.kind.as_str() {
            "paragraph" => (self.paragraph.as_ref()?, ""),
            "heading_1" => (self.heading_1.as_ref()?, "# "),
            "heading_2" => (self.heading_2.as_ref()?, "## "),
            "heading_3" => (self.heading_3.as_ref()?, "### "),
            "bulleted_list_item" => (self.bulleted_list_item.as_ref()?, "• "),
            "numbered_list_item" => (self.numbered_list_item.as_ref()?, "1. "),
            "to_do" => {
                let content = self.to_do.as_ref()?;
                let checkbox = if content.checked { "☑ " } else { "☐ " };
                return Some(format!("{checkbox}{}", plain_text(&content.rich_text)));
            }
            "quote" => (self.quote.as_ref()?, "> "),
            "code" => (self.code.as_ref()?, ""),
            "callout" => (self.callout.as_ref()?, ""),
            _ => return None,
        };
        let text = plain_text(&content.rich_text);
        if text.is_empty() {
            return None;
        }
        Some(format!("{prefix}{text}"))
    }
}

fn plain_text(rich_text: &[RichText]) -> String {
    rich_text
        .iter()
        .map(|part| part.plain_text.as_str())
        .fold(String::new(), |mut text, part| {
            text.push_str(part);
            text
        })
}

#[cfg(test)]
mod tests {
    use super::{Block, PageResponse};

    #[test]
    fn parses_page_title_and_supported_text_blocks() {
        let page: PageResponse =
            serde_json::from_str(r#"{"properties":{"title":{"title":[{"plain_text":"Home"}]}}}"#)
                .expect("page response should parse");
        assert_eq!(page.properties["title"].title[0].plain_text, "Home");

        let block: Block = serde_json::from_str(
            r#"{"type":"paragraph","paragraph":{"rich_text":[{"plain_text":"first"},{"plain_text":" second"}]}}"#,
        )
        .expect("paragraph block should parse");
        assert_eq!(block.display_line().as_deref(), Some("first second"));
    }

    #[test]
    fn renders_to_do_state_and_ignores_unsupported_blocks() {
        let block: Block = serde_json::from_str(
            r#"{"type":"to_do","to_do":{"checked":true,"rich_text":[{"plain_text":"done"}]}}"#,
        )
        .expect("to-do block should parse");
        assert_eq!(block.display_line().as_deref(), Some("☑ done"));

        let block: Block =
            serde_json::from_str(r#"{"type":"divider"}"#).expect("unknown block type should parse");
        assert!(block.display_line().is_none());
    }
}
