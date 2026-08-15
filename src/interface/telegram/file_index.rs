use std::{collections::HashSet, sync::LazyLock};

use serde::{Deserialize, Serialize};
use teloxide::types::{Chat, InlineKeyboardButtonKind, Message, MessageEntityKind};
use tracing::{error, info};
use url::Url;

use crate::{
    application::import::ImportedMedia,
    application::ports::{Message as OutboundMessage, MessageSender},
    error::AppError,
    infrastructure::share::file_parser::ShareFileParser,
    infrastructure::{event_bus::Event, share::is_supported_share_url},
    interface::import::{NO_NEW_MEDIA_MESSAGE, format_imported_media},
};

// --- MediaSource & ProcessMediaSources event ---

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum MediaSource {
    ShareUrl(String),
    Fslink(String),
    TgDocument { file_id: String, file_name: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TelegramSourceContext {
    pub channel_post: bool,
    pub reply_to_message_id: Option<i32>,
    pub source_chat_id: i64,
    pub source_message_id: i32,
    pub source_message_link: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SourceContext {
    #[serde(rename = "telegram")]
    Telegram(TelegramSourceContext),
}

impl SourceContext {
    pub fn channel_post(&self) -> bool {
        match self {
            Self::Telegram(ctx) => ctx.channel_post,
        }
    }

    pub fn reply_to_message_id(&self) -> Option<i32> {
        match self {
            Self::Telegram(ctx) => ctx.reply_to_message_id,
        }
    }

    pub fn source_chat_id(&self) -> i64 {
        match self {
            Self::Telegram(ctx) => ctx.source_chat_id,
        }
    }

    pub fn source_message_id(&self) -> i32 {
        match self {
            Self::Telegram(ctx) => ctx.source_message_id,
        }
    }

    pub fn source_message_link(&self) -> Option<&str> {
        match self {
            Self::Telegram(ctx) => ctx.source_message_link.as_deref(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessMediaSources {
    pub source: MediaSource,
    pub description: Option<String>,
    #[serde(default)]
    pub source_context: Option<SourceContext>,
    #[serde(default)]
    pub channel_post: bool,
    #[serde(default)]
    pub reply_to_message_id: Option<i32>,
}

impl Event for ProcessMediaSources {
    const NAME: &'static str = "ProcessMediaSources";
}

impl MediaSource {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::ShareUrl(_) => "share_url",
            Self::Fslink(_) => "fslink",
            Self::TgDocument { .. } => "tg_document",
        }
    }
}

impl ProcessMediaSources {
    pub fn source_context(&self) -> Option<&SourceContext> {
        self.source_context.as_ref()
    }

    pub fn source_channel_post(&self) -> bool {
        self.source_context
            .as_ref()
            .map(SourceContext::channel_post)
            .unwrap_or(self.channel_post)
    }

    pub fn source_reply_to_message_id(&self) -> Option<i32> {
        self.source_context
            .as_ref()
            .and_then(SourceContext::reply_to_message_id)
            .or(self.reply_to_message_id)
    }

    pub fn source_chat_id(&self) -> Option<i64> {
        self.source_context
            .as_ref()
            .map(SourceContext::source_chat_id)
    }

    pub fn source_message_id(&self) -> Option<i32> {
        self.source_context
            .as_ref()
            .map(SourceContext::source_message_id)
    }

    pub fn source_message_link(&self) -> Option<&str> {
        self.source_context
            .as_ref()
            .and_then(SourceContext::source_message_link)
    }
}

// --- Source extraction ---

pub fn telegram_source_context_from_message(
    msg: &Message,
    channel_post: bool,
    reply_to_message_id: Option<i32>,
) -> SourceContext {
    let source_message_link = match build_source_message_link(&msg.chat, msg.id.0, channel_post) {
        Ok(link) => link,
        Err(err) => {
            error!(
                channel_post,
                source_chat_id = msg.chat.id.0,
                source_message_id = msg.id.0,
                error = %err,
                "Failed to build source message link"
            );
            None
        }
    };

    SourceContext::Telegram(TelegramSourceContext {
        channel_post,
        reply_to_message_id,
        source_chat_id: msg.chat.id.0,
        source_message_id: msg.id.0,
        source_message_link,
    })
}

pub fn extract_media_sources(msg: &Message) -> Vec<MediaSource> {
    let text = msg.text().or(msg.caption()).unwrap_or_default();
    let mut sources = Vec::new();

    // Share URLs
    let urls = extract_urls_from_msg(msg);
    let mut processed_urls = HashSet::new();
    for url in urls {
        if is_supported_share_url(&url) && processed_urls.insert(url.to_string()) {
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
    notify_service: &impl MessageSender,
    source_context: Option<&SourceContext>,
    reply_to: Option<i32>,
    imported: &[ImportedMedia],
) {
    let mut msg_sent = false;
    for media in imported {
        if let Some(summary) = format_imported_media(media) {
            send_notify(
                notify_service,
                source_context,
                reply_to,
                "import_result",
                &summary,
            )
            .await;
            msg_sent = true;
        }
    }
    if !msg_sent {
        send_notify(
            notify_service,
            source_context,
            reply_to,
            "no_new_media",
            NO_NEW_MEDIA_MESSAGE,
        )
        .await;
    }
}

#[cfg(test)]
mod notification_tests {
    use super::{
        SourceContext, TelegramSourceContext, append_source_message_link_suffix,
        format_source_notification_text,
    };

    #[test]
    fn appends_source_message_link_suffix_for_channel_source() {
        let text = append_source_message_link_suffix(
            "开始处理分享: https://115.com/s/share-id?rc=abc",
            Some("https://t.me/c/cookie_gy/123"),
        );

        assert_eq!(
            text,
            "开始处理分享: https://115.com/s/share-id?rc=abc\n\n源消息: https://t.me/c/cookie_gy/123"
        );
    }

    #[test]
    fn does_not_append_source_message_link_suffix_for_dm_source() {
        let text = format_source_notification_text(
            "开始处理分享: https://115.com/s/share-id?rc=abc",
            Some(&SourceContext::Telegram(TelegramSourceContext {
                channel_post: false,
                reply_to_message_id: Some(7),
                source_chat_id: 42,
                source_message_id: 7,
                source_message_link: None,
            })),
        );

        assert_eq!(text, "开始处理分享: https://115.com/s/share-id?rc=abc");
    }

    #[test]
    fn appends_source_message_link_suffix_for_channel_notification_formatting() {
        let text = format_source_notification_text(
            "开始处理分享: https://115.com/s/share-id?rc=abc",
            Some(&SourceContext::Telegram(TelegramSourceContext {
                channel_post: true,
                reply_to_message_id: None,
                source_chat_id: -1001234567890,
                source_message_id: 321,
                source_message_link: Some("https://t.me/c/1234567890/321".to_string()),
            })),
        );

        assert_eq!(
            text,
            "开始处理分享: https://115.com/s/share-id?rc=abc\n\n源消息: https://t.me/c/1234567890/321"
        );
    }
}

pub async fn send_import_error(
    notify_service: &impl MessageSender,
    source_context: Option<&SourceContext>,
    reply_to: Option<i32>,
    prefix: &str,
    error: &AppError,
) {
    let suffix = match error {
        AppError::InvalidParameter(_) => format!("参数错误：{error}"),
        AppError::NotFound(_) => format!("未找到资源：{error}"),
        AppError::Unauthorized(_) => format!("未授权：{error}"),
        AppError::Database(_, _) => format!("数据库错误：{error}"),
        AppError::ExternalService(_, _) => format!("外部服务失败：{error}"),
        AppError::Network(_, _) => format!("网络错误：{error}"),
        AppError::Internal(_) => format!("系统错误：{error}"),
    };
    send_notify(
        notify_service,
        source_context,
        reply_to,
        "import_error",
        format!("{prefix}: {suffix}"),
    )
    .await;
}

pub(crate) async fn send_observation_notification(
    notify_service: &impl MessageSender,
    source_context: Option<&SourceContext>,
    reply_to: Option<i32>,
    notification_kind: &'static str,
    text: impl Into<String>,
) {
    send_notify(
        notify_service,
        source_context,
        reply_to,
        notification_kind,
        text,
    )
    .await;
}

async fn send_notify(
    notify_service: &impl MessageSender,
    source_context: Option<&SourceContext>,
    reply_to: Option<i32>,
    notification_kind: &'static str,
    text: impl Into<String>,
) {
    let text = format_source_notification_text(text, source_context);
    let has_source_message_link = source_context
        .and_then(SourceContext::source_message_link)
        .is_some();
    info!(
        notification_kind,
        reply_to = ?reply_to,
        channel_post = source_context.is_some_and(SourceContext::channel_post),
        source_chat_id = ?source_context.map(SourceContext::source_chat_id),
        source_message_id = ?source_context.map(SourceContext::source_message_id),
        source_message_link = ?source_context.and_then(SourceContext::source_message_link),
        has_source_message_link,
        "Sending Telegram observation notification"
    );
    if let Err(e) = notify_service
        .send(&OutboundMessage::new(text, reply_to))
        .await
    {
        tracing::error!(
            notification_kind,
            reply_to = ?reply_to,
            "Failed publish send telegram message event: {e}"
        );
    }
}

pub(crate) fn format_source_notification_text(
    text: impl Into<String>,
    source_context: Option<&SourceContext>,
) -> String {
    let source_message_link = source_context.and_then(SourceContext::source_message_link);
    append_source_message_link_suffix(text, source_message_link)
}

fn append_source_message_link_suffix(
    text: impl Into<String>,
    source_message_link: Option<&str>,
) -> String {
    let text = text.into();
    let Some(source_message_link) = source_message_link else {
        return text;
    };

    format!("{text}\n\n源消息: {source_message_link}")
}

fn build_source_message_link(
    chat: &Chat,
    message_id: i32,
    channel_post: bool,
) -> Result<Option<String>, String> {
    if !channel_post {
        return Ok(None);
    }

    if let Some(username) = chat.username() {
        return Ok(Some(format!("https://t.me/{username}/{message_id}")));
    }

    let internal_channel_id = chat
        .id
        .0
        .checked_abs()
        .and_then(|value| value.checked_sub(1_000_000_000_000))
        .filter(|value| *value > 0)
        .ok_or_else(|| format!("invalid private channel chat id: {}", chat.id.0))?;

    Ok(Some(format!(
        "https://t.me/c/{internal_channel_id}/{message_id}"
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use teloxide::types::Message;

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

    #[test]
    fn extract_media_sources_ignores_unsupported_links() {
        let msg: Message = serde_json::from_value(json!({
            "message_id": 1,
            "date": 1_700_000_000,
            "chat": {
                "id": 42,
                "type": "private"
            },
            "text": "https://www.themoviedb.org/tv/314784"
        }))
        .unwrap();

        assert!(extract_media_sources(&msg).is_empty());
    }

    #[test]
    fn extract_media_sources_keeps_supported_share_links_and_deduplicates_them() {
        let msg: Message = serde_json::from_value(json!({
            "message_id": 1,
            "date": 1_700_000_000,
            "chat": {
                "id": 42,
                "type": "private"
            },
            "text": "https://115.com/s/share-id?rc=abc\nhttps://115.com/s/share-id?rc=abc\nhttps://www.themoviedb.org/tv/314784"
        }))
        .unwrap();

        assert_eq!(
            extract_media_sources(&msg),
            vec![MediaSource::ShareUrl(
                "https://115.com/s/share-id?rc=abc".to_string()
            )]
        );
    }

    #[test]
    fn process_media_sources_deserializes_legacy_payload_shape() {
        let payload: ProcessMediaSources = serde_json::from_value(json!({
            "source": {
                "ShareUrl": "https://115.com/s/share-id?rc=abc"
            },
            "description": "资源说明",
            "channel_post": true,
            "reply_to_message_id": null
        }))
        .unwrap();

        assert!(matches!(
            payload.source,
            MediaSource::ShareUrl(ref url) if url == "https://115.com/s/share-id?rc=abc"
        ));
        assert_eq!(payload.description.as_deref(), Some("资源说明"));
        assert!(payload.channel_post);
        assert_eq!(payload.reply_to_message_id, None);
    }

    #[test]
    fn process_media_sources_deserializes_new_telegram_source_context_shape() {
        let payload: ProcessMediaSources = serde_json::from_value(json!({
            "source": {
                "ShareUrl": "https://115.com/s/share-id?rc=abc"
            },
            "description": "资源说明",
            "source_context": {
                "telegram": {
                    "channel_post": true,
                    "reply_to_message_id": null,
                    "source_chat_id": -1001234567890i64,
                    "source_message_id": 321,
                    "source_message_link": "https://t.me/c/1234567890/321"
                }
            }
        }))
        .unwrap();

        assert!(matches!(
            payload.source,
            MediaSource::ShareUrl(ref url) if url == "https://115.com/s/share-id?rc=abc"
        ));
        assert_eq!(payload.description.as_deref(), Some("资源说明"));
    }

    #[test]
    fn builds_public_channel_source_message_link() {
        let msg: Message = serde_json::from_value(json!({
            "message_id": 321,
            "date": 1_700_000_000,
            "chat": {
                "id": -1001234567890i64,
                "title": "公开频道",
                "username": "cookie_gy",
                "type": "channel"
            },
            "sender_chat": {
                "id": -1001234567890i64,
                "title": "公开频道",
                "username": "cookie_gy",
                "type": "channel"
            }
        }))
        .unwrap();

        let context = telegram_source_context_from_message(&msg, true, None);

        assert_eq!(
            context.source_message_link(),
            Some("https://t.me/cookie_gy/321")
        );
    }

    #[test]
    fn builds_private_channel_source_message_link() {
        let msg: Message = serde_json::from_value(json!({
            "message_id": 321,
            "date": 1_700_000_000,
            "chat": {
                "id": -1001234567890i64,
                "title": "私有频道",
                "type": "channel"
            },
            "sender_chat": {
                "id": -1001234567890i64,
                "title": "私有频道",
                "type": "channel"
            }
        }))
        .unwrap();

        let context = telegram_source_context_from_message(&msg, true, None);

        assert_eq!(
            context.source_message_link(),
            Some("https://t.me/c/1234567890/321")
        );
    }
}
