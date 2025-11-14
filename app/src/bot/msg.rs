use std::{collections::HashSet, sync::LazyLock};

use reqwest::Url;
use teloxide::{
    net::Download,
    prelude::*,
    sugar::request::RequestReplyExt,
    types::{Document, InlineKeyboardButtonKind, MessageEntityKind},
};
use tracing::error;

use crate::{
    library::{self, ImportSummary, ShareUrl},
    log_time,
    state::AppState,
};

static URL_RE: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"https?://[^\s/$.?#].[^\s]*").expect("Failed to compile URL regex"));

pub(super) struct MsgProcessor<'a> {
    pub state: &'a AppState,
    pub bot: &'a Bot,
    pub msg: &'a Message,
}

impl MsgProcessor<'_> {
    pub(super) async fn process(&self) -> ResponseResult<()> {
        log_time!("process telegram message");

        if let Some(doc) = self.msg.document() {
            return self.handle_document(doc).await;
        }

        let mut processed_urls = HashSet::new();
        let urls = self.extract_urls();
        for url in &urls {
            if let Some(share_url) = ShareUrl::from(url)
                && processed_urls.insert(share_url.get_url().to_string())
            {
                // 避免重复处理相同的 URL
                self.handle_share_url(&share_url).await?;
            }
        }

        let fslinks = self.extract_fslink();
        for fslink in fslinks {
            self.handle_fslink(fslink).await?;
        }

        Ok(())
    }

    async fn handle_document(&self, doc: &Document) -> ResponseResult<()> {
        if !doc.file_name.as_ref().is_some_and(|n| n.ends_with(".json")) {
            self.send_message("不是 JSON 文件，忽略").await?;
            return Ok(());
        }

        self.send_message("开始处理 JSON 文件").await?;

        let file = self.bot.get_file(doc.file.id.to_owned()).await?;
        let mut content = Vec::new();
        self.bot.download_file(&file.path, &mut content).await?;

        match library::import_from_json(self.state, content).await {
            Ok(summary) => {
                let formatted: String = self.format_import_summary(&summary);
                self.send_message(formatted).await?;
            }
            Err(e) => {
                error!("import from json failed: {}", e);
                self.send_message(format!("JSON 文件处理失败: {}", e)).await?;
            }
        }

        Ok(())
    }

    async fn handle_share_url(&self, url: &ShareUrl<'_>) -> ResponseResult<()> {
        let reply = format!("开始处理分享: {}", url.get_url());
        self.send_message(&reply).await?;

        match library::import_from_share_url(self.state, url).await {
            Ok(summary) => {
                let formatted: String = self.format_import_summary(&summary);
                self.send_message(formatted).await?;
            }
            Err(e) => {
                error!("import from share url {} failed: {}", url.get_url(), e);
                self.send_message(format!("分享处理失败: {}", e)).await?;
            }
        }

        Ok(())
    }

    async fn handle_fslink(&self, fslink: &str) -> ResponseResult<()> {
        self.send_message("开始处理秒传").await?;

        match library::import_from_fslink(self.state, fslink).await {
            Ok(summary) => {
                let formatted: String = self.format_import_summary(&summary);
                self.send_message(formatted).await?;
            }
            Err(e) => {
                error!("import from fslink {} failed: {}", fslink, e);
                self.send_message(format!("秒传处理失败: {}", e)).await?;
            }
        }

        Ok(())
    }

    fn extract_fslink(&self) -> Vec<&str> {
        let text = self.msg.text().or(self.msg.caption()).unwrap_or_default();
        text.lines().filter(|line| library::is_fslink(line)).collect()
    }

    fn extract_urls(&self) -> Vec<Url> {
        let mut urls = Vec::new();

        let text = self.msg.text().or(self.msg.caption()).unwrap_or_default();
        self.extract_urls_from_text(text, &mut urls);

        if let Some(entities) = self.msg.caption_entities() {
            for entity in entities {
                if let MessageEntityKind::TextLink { url } = &entity.kind {
                    urls.push(url.clone());
                }
            }
        }

        if let Some(reply_markup) = self.msg.reply_markup() {
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

    fn extract_urls_from_text(&self, text: &str, urls: &mut Vec<Url>) {
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

    async fn send_message<T: Into<String>>(&self, text: T) -> ResponseResult<Message> {
        if self.msg.from.is_none() {
            self.bot.send_message(self.get_chat_id(), text).await
        } else {
            self.bot
                .send_message(self.get_chat_id(), text)
                .reply_to(self.msg.id)
                .await
        }
    }

    #[inline]
    fn get_chat_id(&self) -> ChatId {
        ChatId(self.state.config.get_telegram_config().user_id)
    }

    fn format_import_summary(&self, summary: &ImportSummary) -> String {
        let total_size_gb = summary.total_size as f64 / 1_000_000_000.0;
        let avg_size_gb = if summary.success > 0 {
            summary.total_size as f64 / summary.success as f64 / 1_000_000_000.0
        } else {
            0.0
        };
        format!(
            "📁 共 {} 个文件\n\
             ✅ 成功: {}个\n\
             ❌ 失败: {}个\n\
             🔄 跳过文件: {}个\n\
             📊 成功转存大小: {:.2} GB\n\
             📊 平均文件大小: {:.2} GB\n\
             ⏱️ 耗时: {:.2} 秒",
            summary.total,
            summary.success,
            summary.failed,
            summary.skipped,
            total_size_gb,
            avg_size_gb,
            summary.cost.as_secs_f64()
        )
    }
}
