use std::{collections::HashSet, sync::LazyLock};

use serde::{Deserialize, Serialize};
use teloxide::types::{InlineKeyboardButtonKind, Message, MessageEntityKind};
use url::Url;

use crate::{
    application::import::ImportedMedia,
    error::AppError,
    infrastructure::event_bus::Event,
    infrastructure::share::file_parser::ShareFileParser,
    interface::import::{NO_NEW_MEDIA_MESSAGE, format_imported_media},
};

use super::NotifyService;

// --- MediaSource & ProcessMediaSources event ---

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MediaSource {
    ShareUrl(String),
    Fslink(String),
    TgDocument { file_id: String, file_name: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessMediaSources {
    pub source: MediaSource,
    pub description: Option<String>,
    pub channel_post: bool,
    pub reply_to_message_id: Option<i32>,
}

impl Event for ProcessMediaSources {
    const NAME: &'static str = "ProcessMediaSources";
}

// --- Source extraction ---

pub fn extract_media_sources(msg: &Message) -> Vec<MediaSource> {
    let text = msg.text().or(msg.caption()).unwrap_or_default();
    let mut sources = Vec::new();

    // Share URLs
    let urls = extract_urls_from_msg(msg);
    let mut processed_urls = HashSet::new();
    for url in urls {
        if processed_urls.insert(url.to_string()) {
            sources.push(MediaSource::ShareUrl(url.to_string()));
        }
    }

    // Fslines
    for line in text.lines() {
        if ShareFileParser::is_fslink(line) {
            sources.push(MediaSource::Fslink(line.to_owned()));
        }
    }

    // Documents (.json/.cas)
    if let Some(doc) = msg.document()
        && let Some(file_name) = &doc.file_name
        && (file_name.ends_with(".json") || file_name.ends_with(".cas"))
    {
        sources.push(MediaSource::TgDocument {
            file_id: doc.file.id.to_string(),
            file_name: file_name.clone(),
        });
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
        .filter(|line| !ShareFileParser::is_fslink(line))
        .collect::<Vec<_>>()
        .join("\n");
    let description = description.trim();
    (!description.is_empty()).then_some(description.to_owned())
}

// --- URL extraction (moved from msg.rs) ---

static URL_RE: LazyLock<regex::Regex> = LazyLock::new(|| {
    regex::Regex::new(r"https?://[^\s/$.?#].[^\s]*").expect("Failed to compile URL regex")
});

pub fn extract_urls_from_msg(msg: &Message) -> Vec<Url> {
    let mut urls = Vec::new();

    if let Some(text) = msg.text() {
        extract_urls_from_text(text, &mut urls);
    }
    if let Some(caption) = msg.caption() {
        extract_urls_from_text(caption, &mut urls);
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

fn extract_urls_from_text(text: &str, urls: &mut Vec<Url>) {
    if text.is_empty() {
        return;
    }

    for cap in URL_RE.captures_iter(text) {
        if let Some(matched_url) = cap.get(0)
            && let Ok(url) = Url::parse(matched_url.as_str())
        {
            urls.push(url);
        }
    }
}

// --- Notification helpers ---

pub async fn send_import_results(
    notify_service: &NotifyService,
    reply_to: Option<i32>,
    imported: &[ImportedMedia],
) {
    let mut msg_sent = false;
    for media in imported {
        if let Some(summary) = format_imported_media(media) {
            send_notify(notify_service, reply_to, &summary).await;
            msg_sent = true;
        }
    }
    if !msg_sent {
        send_notify(notify_service, reply_to, NO_NEW_MEDIA_MESSAGE).await;
    }
}

pub async fn send_import_error(
    notify_service: &NotifyService,
    reply_to: Option<i32>,
    prefix: &str,
    error: &AppError,
) {
    let suffix = match error {
        AppError::InvalidParameter(_) => format!("参数错误：{error}"),
        AppError::NotFound(_) => format!("未找到资源：{error}"),
        AppError::Database(_, _) => format!("数据库错误：{error}"),
        AppError::ExternalService(_, _) => format!("外部服务失败：{error}"),
        AppError::Network(_, _) => format!("网络错误：{error}"),
        AppError::Internal(_) => format!("系统错误：{error}"),
    };
    send_notify(notify_service, reply_to, format!("{prefix}: {suffix}")).await;
}

async fn send_notify(
    notify_service: &NotifyService,
    reply_to: Option<i32>,
    text: impl Into<String>,
) {
    if let Err(e) = notify_service.send_message(text, reply_to).await {
        tracing::error!("Failed publish send telegram message event: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
