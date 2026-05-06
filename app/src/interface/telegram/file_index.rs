use serde::{Deserialize, Serialize};
use teloxide::types::{InlineKeyboardButtonKind, Message, MessageEntityKind};
use url::Url;

use crate::{application::file_index::FileIndexSource, infrastructure::event_bus::Event};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexFilesFromSource {
    pub sources: Vec<FileIndexSource>,
    pub description: Option<String>,
    pub source_kind: String,
}

impl Event for IndexFilesFromSource {
    const NAME: &'static str = "IndexFilesFromSource";
}

pub fn extract_index_sources(msg: &Message) -> Vec<FileIndexSource> {
    let text = msg.text().or(msg.caption()).unwrap_or_default();
    let urls = extract_urls(msg)
        .into_iter()
        .map(|url| url.to_string())
        .collect::<Vec<_>>();
    extract_index_sources_from_parts(text, urls)
}

pub fn extract_index_sources_from_parts(text: &str, raw_urls: Vec<String>) -> Vec<FileIndexSource> {
    let mut sources = Vec::new();
    for line in text.lines() {
        if crate::application::import::is_fslink(line) {
            sources.push(FileIndexSource::Fslink(line.to_owned()));
        }
    }
    for raw_url in raw_urls {
        let Ok(url) = Url::parse(&raw_url) else {
            continue;
        };
        if crate::application::import::ShareUrl::from(&url).is_some() {
            sources.push(FileIndexSource::ShareUrl(url.to_string()));
        }
    }
    sources
}

pub fn message_description(msg: &Message) -> Option<String> {
    msg.text()
        .or(msg.caption())
        .and_then(message_description_from_text)
}

fn message_description_from_text(text: &str) -> Option<String> {
    let description = text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .filter(|line| !crate::application::import::is_fslink(line))
        .collect::<Vec<_>>()
        .join("\n");
    let description = description.trim();
    (!description.is_empty()).then_some(description.to_owned())
}

fn extract_urls(msg: &Message) -> Vec<Url> {
    let mut urls = Vec::new();
    if let Some(text) = msg.text() {
        super::msg::extract_urls_from_text(text, &mut urls);
    }
    if let Some(caption) = msg.caption() {
        super::msg::extract_urls_from_text(caption, &mut urls);
    }

    if let Some(entities) = msg.caption_entities() {
        for entity in entities {
            if let MessageEntityKind::TextLink { url } = &entity.kind {
                urls.push(url.clone());
            }
        }
    }

    if let Some(reply_markup) = msg.reply_markup() {
        for buttons in &reply_markup.inline_keyboard {
            for button in buttons {
                if let InlineKeyboardButtonKind::Url(url) = &button.kind {
                    urls.push(url.clone());
                }
            }
        }
    }
    urls
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_index_sources_from_text_parts_keeps_share_and_fslink() {
        let sources = extract_index_sources_from_parts(
            "123FSLinkV2$aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa#100#movie.mkv",
            vec!["https://www.123pan.com/s/test?pwd=pass".to_owned()],
        );

        assert_eq!(sources.len(), 2);
    }

    #[test]
    fn message_description_ignores_fslink_lines() {
        let description = message_description_from_text(
            "资源说明\n123FSLinkV2$aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa#100#movie.mkv",
        );

        assert_eq!(description.as_deref(), Some("资源说明"));
    }

    #[test]
    fn message_description_returns_none_for_fslink_only_text() {
        let description = message_description_from_text(
            "123FSLinkV2$aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa#100#movie.mkv",
        );

        assert_eq!(description, None);
    }
}
