use std::{collections::HashSet, sync::LazyLock};

use reqwest::Url;
use teloxide::{
    net::Download,
    prelude::*,
    types::{Document, InlineKeyboardButtonKind, MessageEntityKind},
};
use tracing::{error, info};

use super::{ImportService, NotifyService, format::format_imported_media};
use crate::{
    library::{self, ImportedMedia, ShareUrl},
    log_time,
};

static URL_RE: LazyLock<regex::Regex> = LazyLock::new(|| {
    regex::Regex::new(r"https?://[^\s/$.?#].[^\s]*").expect("Failed to compile URL regex")
});

pub(super) struct MsgProcessor<'a> {
    pub import_service: &'a ImportService,
    pub notify_service: &'a NotifyService,
    pub bot: &'a Bot,
    pub msg: &'a Message,
    pub from_monitor: bool,
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
        if !doc.file_name.as_ref().is_some_and(|n| n.ends_with(".json")) && !self.from_monitor {
            self.send_message("不是 JSON 文件，忽略").await;
            return Ok(());
        }

        if !self.from_monitor {
            self.send_message("开始处理 JSON 文件").await;
        }

        let file = self.bot.get_file(doc.file.id.to_owned()).await?;
        if file.meta.size > 10 * 1024 * 1024 {
            self.send_message("文件过大，无法处理").await;
            return Ok(());
        }

        let mut content = Vec::with_capacity(file.meta.size.try_into().unwrap());
        self.bot.download_file(&file.path, &mut content).await?;

        match self.import_service.import_from_json(content).await {
            Ok(imported) => {
                self.handle_imported_medias(imported).await?;
            }
            Err(e) => {
                error!("import from json failed: {}", e);
                self.send_message(format!("JSON 文件处理失败: {}", e)).await;
            }
        }

        Ok(())
    }

    async fn handle_share_url(&self, url: &ShareUrl<'_>) -> ResponseResult<()> {
        if !self.from_monitor {
            let reply = format!("开始处理分享: {}", url.get_url());
            self.send_message(&reply).await;
        }

        match self.import_service.import_from_share_url(url).await {
            Ok(imported) => {
                self.handle_imported_medias(imported).await?;
            }
            Err(e) => {
                error!("import from share url {} failed: {}", url.get_url(), e);
                self.send_message(format!("分享处理失败: {}", e)).await;
            }
        }

        Ok(())
    }

    async fn handle_fslink(&self, fslink: &str) -> ResponseResult<()> {
        if !self.from_monitor {
            self.send_message("开始处理秒传").await;
        }

        match self.import_service.import_from_fslink(fslink).await {
            Ok(imported) => {
                self.handle_imported_medias(imported).await?;
            }
            Err(e) => {
                error!("import from fslink {} failed: {}", fslink, e);
                self.send_message(format!("秒传处理失败: {}", e)).await;
            }
        }

        Ok(())
    }

    fn extract_fslink(&self) -> Vec<&str> {
        let text = self.msg.text().or(self.msg.caption()).unwrap_or_default();
        text.lines()
            .filter(|line| library::is_fslink(line))
            .collect()
    }

    fn extract_urls(&self) -> Vec<Url> {
        let mut urls = Vec::new();

        let text = self.msg.text().unwrap_or_default();
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

    async fn handle_imported_medias(&self, imported: Vec<ImportedMedia>) -> ResponseResult<()> {
        let mut msg_sent = false;
        for media in &imported {
            info!("Imported media: {:?}", media);
            if let Some(summary) = format_imported_media(media) {
                self.send_message(summary).await;
                msg_sent = true;
            }
        }

        if !self.from_monitor && !msg_sent {
            self.send_message("没有新入库的媒体").await;
        }
        Ok(())
    }

    async fn send_message<T: Into<String>>(&self, text: T) {
        let reply_to = self.msg.from.as_ref().map(|_| self.msg.id.0);
        if let Err(e) = self.notify_service.send_message(text, reply_to).await {
            error!("Failed publish send telegram message event: {}", e);
        }
    }
}
