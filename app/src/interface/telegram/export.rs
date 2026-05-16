use serde::Deserialize;
use url::Url;

use crate::{
    infrastructure::share::{file_parser::ShareFileParser, is_supported_share_url},
    interface::telegram::file_index::MediaSource,
};

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum ExportText {
    Text(String),
    Parts(Vec<ExportTextPart>),
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum ExportTextPart {
    Text(String),
    Entity(ExportEntity),
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ExportEntity {
    #[serde(rename = "type")]
    pub entity_type: String,
    pub text: String,
    #[serde(default)]
    pub href: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ExportInlineButton {
    #[serde(rename = "type")]
    pub button_type: String,
    #[allow(dead_code)]
    pub text: String,
    #[serde(default)]
    pub data: Option<String>,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub href: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ExportMessage {
    #[allow(dead_code)]
    pub id: i64,
    #[serde(default)]
    pub text: Option<ExportText>,
    #[serde(default)]
    pub text_entities: Vec<ExportEntity>,
    #[serde(default)]
    pub inline_bot_buttons: Vec<Vec<ExportInlineButton>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ExportRoot {
    pub messages: Vec<ExportMessage>,
}

pub fn extract_media_sources(msg: &ExportMessage) -> Vec<MediaSource> {
    let mut sources = Vec::new();
    let mut seen_urls = std::collections::HashSet::new();

    for url in extract_urls(msg) {
        if is_supported_share_url(&url) && seen_urls.insert(url.as_str().to_owned()) {
            sources.push(MediaSource::ShareUrl(url.to_string()));
        }
    }

    collect_fslinks_from_text(&normalized_text(msg), &mut sources);
    collect_fslinks_from_fragments(msg, &mut sources);

    sources
}

pub fn message_description(msg: &ExportMessage) -> Option<String> {
    let description = normalized_text(msg)
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .filter(|line| !ShareFileParser::is_fslink(line))
        .filter(|line| !line_contains_supported_share_url(line))
        .collect::<Vec<_>>()
        .join("\n");
    let description = description.trim();
    (!description.is_empty()).then_some(description.to_owned())
}

fn extract_urls(msg: &ExportMessage) -> Vec<Url> {
    let mut urls = Vec::new();

    for entity in &msg.text_entities {
        push_entity_url(entity, &mut urls);
    }

    if let Some(text) = &msg.text {
        collect_urls_from_text(text, &mut urls);
    }

    for row in &msg.inline_bot_buttons {
        for button in row {
            if button.button_type == "url"
                && let Some(raw) = button
                    .data
                    .as_deref()
                    .or(button.url.as_deref())
                    .or(button.href.as_deref())
                && let Ok(url) = Url::parse(raw)
            {
                urls.push(url);
            }
        }
    }

    urls
}

fn push_entity_url(entity: &ExportEntity, urls: &mut Vec<Url>) {
    match entity.entity_type.as_str() {
        "text_link" => {
            if let Some(raw) = entity.href.as_deref()
                && let Ok(url) = Url::parse(raw)
            {
                urls.push(url);
            }
        }
        "link" => {
            if let Ok(url) = Url::parse(entity.text.as_str()) {
                urls.push(url);
            }
        }
        _ => {}
    }
}

fn collect_urls_from_text(text: &ExportText, urls: &mut Vec<Url>) {
    match text {
        ExportText::Text(value) => collect_urls_from_plain_text(value, urls),
        ExportText::Parts(parts) => {
            for part in parts {
                match part {
                    ExportTextPart::Text(value) => collect_urls_from_plain_text(value, urls),
                    ExportTextPart::Entity(entity) => push_entity_url(entity, urls),
                }
            }
        }
    }
}

fn collect_urls_from_plain_text(text: &str, urls: &mut Vec<Url>) {
    for cap in URL_RE.captures_iter(text) {
        if let Some(matched_url) = cap.get(0)
            && let Ok(url) = Url::parse(matched_url.as_str())
        {
            urls.push(url);
        }
    }
}

fn line_contains_supported_share_url(line: &str) -> bool {
    let mut urls = Vec::new();
    collect_urls_from_plain_text(line, &mut urls);
    urls.into_iter().any(|url| is_supported_share_url(&url))
}

fn normalized_text(msg: &ExportMessage) -> String {
    match &msg.text {
        None => String::new(),
        Some(ExportText::Text(value)) => value.clone(),
        Some(ExportText::Parts(parts)) => parts
            .iter()
            .map(|part| match part {
                ExportTextPart::Text(value) => value.clone(),
                ExportTextPart::Entity(entity) => entity.text.clone(),
            })
            .collect::<Vec<_>>()
            .join(""),
    }
}

fn collect_fslinks_from_fragments(msg: &ExportMessage, sources: &mut Vec<MediaSource>) {
    let mut seen = std::collections::HashSet::new();

    if let Some(text) = &msg.text {
        collect_fslinks_from_export_text(text, &mut seen, sources);
    }

    for entity in &msg.text_entities {
        collect_fslinks_from_text(&entity.text, sources);
    }
}

fn collect_fslinks_from_export_text(
    text: &ExportText,
    seen: &mut std::collections::HashSet<String>,
    sources: &mut Vec<MediaSource>,
) {
    match text {
        ExportText::Text(value) => collect_fslinks_from_text_with_seen(value, seen, sources),
        ExportText::Parts(parts) => {
            for part in parts {
                match part {
                    ExportTextPart::Text(value) => {
                        collect_fslinks_from_text_with_seen(value, seen, sources)
                    }
                    ExportTextPart::Entity(entity) => {
                        collect_fslinks_from_text_with_seen(&entity.text, seen, sources)
                    }
                }
            }
        }
    }
}

fn collect_fslinks_from_text(text: &str, sources: &mut Vec<MediaSource>) {
    let mut seen = std::collections::HashSet::new();
    collect_fslinks_from_text_with_seen(text, &mut seen, sources);
}

fn collect_fslinks_from_text_with_seen(
    text: &str,
    seen: &mut std::collections::HashSet<String>,
    sources: &mut Vec<MediaSource>,
) {
    for line in text.lines() {
        let line = line.trim();
        if ShareFileParser::is_fslink(line) && seen.insert(line.to_owned()) {
            sources.push(MediaSource::Fslink(line.to_owned()));
        }
    }
}

static URL_RE: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
    regex::Regex::new(r"https?://[^\s/$.?#].[^\s]*").expect("Failed to compile URL regex")
});

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn extracts_sources_from_export_message() {
        let msg: ExportMessage = serde_json::from_value(json!({
            "id": 1,
            "text": [
                {"type": "text_link", "text": "查看链接", "href": "https://pan.quark.cn/s/share-id?pwd=abc"},
                " 123FSLinkV2$aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa#100#movie.mkv "
            ],
            "text_entities": [
                {"type": "text_link", "text": "查看链接", "href": "https://pan.quark.cn/s/share-id?pwd=abc"},
                {"type": "link", "text": "https://pan.quark.cn/s/share-id?pwd=abc"}
            ],
            "inline_bot_buttons": [
                [
                    {"type": "url", "text": "进入频道 📺", "data": "https://t.me/cookie_gy"}
                ]
            ]
        }))
        .unwrap();

        assert_eq!(
            extract_media_sources(&msg),
            vec![
                MediaSource::ShareUrl("https://pan.quark.cn/s/share-id?pwd=abc".to_string()),
                MediaSource::Fslink(
                    "123FSLinkV2$aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa#100#movie.mkv".to_string()
                ),
            ]
        );
    }

    #[test]
    fn extracts_description_without_fslink_lines() {
        let msg: ExportMessage = serde_json::from_value(json!({
            "id": 1,
            "text": "资源说明\n123FSLinkV2$aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa#100#movie.mkv"
        }))
        .unwrap();

        assert_eq!(message_description(&msg).as_deref(), Some("资源说明"));
    }
}
