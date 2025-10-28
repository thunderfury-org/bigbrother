use reqwest::Url;
use serde::Deserialize;
use teloxide::types::{Document, MessageEntityKind};
use teloxide::{net::Download, prelude::*, sugar::request::RequestReplyExt};

use crate::{client::RequestError, state::AppState};

pub async fn run_bot(state: AppState) {
    let bot = Bot::new(state.config.get_telegram_config().bot_token.as_str());
    let handler = dptree::entry()
        .branch(Update::filter_channel_post().endpoint(handle_channel_post))
        .branch(Update::filter_message().endpoint(handle_message));

    Dispatcher::builder(bot, handler)
        .enable_ctrlc_handler()
        .dependencies(dptree::deps![state])
        .build()
        .dispatch()
        .await;
}

async fn handle_channel_post(state: AppState, bot: Bot, msg: Message) -> ResponseResult<()> {
    if msg
        .caption()
        .is_some_and(|c| c.contains("天地剑心") || c.contains("红石榴餐厅") || c.contains("暗河传"))
    {
        handle_message(state, bot, msg).await?;
    }
    Ok(())
}

async fn handle_message(state: AppState, bot: Bot, msg: Message) -> ResponseResult<()> {
    if let Some(text) = msg.text() {
        if text == "/start" {
            bot.send_message(
                get_chat_id(&state),
                "欢迎使用秒传链接转存机器人！\n\
                 请发送包含秒传链接的 JSON 文件，我将帮助您将文件转存到您的 pan123 账号中。",
            )
            .await?;
        }
    } else if let Some(doc) = msg.document() {
        handle_document(state, bot, doc, &msg).await?;
    } else {
        let urls = get_urls_from_msg(&msg);
        if let Some(url) = urls.first() {
            handle_share_url(state, bot, url, &msg).await?;
        }
    }

    Ok(())
}

fn get_urls_from_msg(msg: &Message) -> Vec<Url> {
    let mut urls = Vec::new();
    if let Some(entities) = msg.caption_entities() {
        for entity in entities {
            match &entity.kind {
                MessageEntityKind::TextLink { url } => {
                    if url
                        .host_str()
                        .is_some_and(|h| h.starts_with("www.123") && h.ends_with(".com"))
                        && url.path().starts_with("/s/")
                    {
                        urls.push(url.clone());
                    }
                }
                _ => {}
            }
        }
    }
    urls
}

async fn handle_share_url(state: AppState, bot: Bot, url: &Url, msg: &Message) -> ResponseResult<()> {
    bot.send_message(get_chat_id(&state), format!("开始处理分享链接: {}", url))
        .await?;

    let share_key = url
        .path_segments()
        .map(|s| s.last().unwrap_or_default())
        .unwrap_or_default();
    let share_password = url
        .query_pairs()
        .find(|(k, _)| k == "pwd")
        .map(|(_, v)| v.to_string())
        .unwrap_or_default();

    let files = state
        .pan123
        .list_share_file(share_key, share_password.as_str(), 0)
        .await
        .unwrap_or_default();

    for file in files {
        println!("Processing file: {:?}", file);
    }

    Ok(())
}

async fn handle_document(state: AppState, bot: Bot, doc: &Document, msg: &Message) -> ResponseResult<()> {
    if !doc.file_name.as_ref().is_some_and(|n| n.ends_with(".json")) {
        bot.send_message(get_chat_id(&state), "不是 json 文件，忽略").await?;
        return Ok(());
    }

    bot.send_message(get_chat_id(&state), "开始处理").await?;
    let file = bot.get_file(doc.file.id.to_owned()).await?;
    let mut content = Vec::new();
    bot.download_file(&file.path, &mut content).await?;

    match serde_json::from_slice::<ResourceExportJson>(&content) {
        Ok(export) => {
            let start = std::time::Instant::now();
            let mut result = ProcessResult::default();

            for f in export.files {
                result.total += 1;
                let path = std::path::Path::new(f.path.as_str())
                    .file_name()
                    .unwrap()
                    .to_str()
                    .unwrap()
                    .to_string();
                let r = state
                    .pan123
                    .fast_upload(
                        state.config.get_pan123_config().file_id,
                        path.as_str(),
                        f.etag.as_str(),
                        f.size,
                    )
                    .await;
                match r {
                    Ok(_) => {
                        result.success += 1;
                        result.total_size += f.size;
                    }
                    Err(RequestError::AlreadyExists) => {
                        result.skipped += 1;
                    }
                    Err(e) => {
                        result.failed += 1;
                        bot.send_message(get_chat_id(&state), format!("上传文件 {} 失败: {}", path, e))
                            .await?;
                    }
                }
            }

            result.cost = start.elapsed();
            bot.send_message(get_chat_id(&state), result.to_string()).await?;
        }
        Err(e) => {
            bot.send_message(get_chat_id(&state), format!("json 解析错误: {}", e))
                .await?;
        }
    }

    Ok(())
}

fn get_chat_id(state: &AppState) -> ChatId {
    ChatId(state.config.get_telegram_config().user_id)
}

#[derive(Debug, Default)]
struct ProcessResult {
    pub success: u32,
    pub failed: u32,
    pub skipped: u32,
    pub total: u32,
    pub total_size: u64,
    pub cost: std::time::Duration,
}

impl ProcessResult {
    fn to_string(&self) -> String {
        let total_size_gb = self.total_size as f64 / 1_000_000_000.0;
        let avg_size_gb = if self.success > 0 {
            self.total_size as f64 / self.success as f64 / 1_000_000_000.0
        } else {
            0.0
        };
        format!(
            "✅秒传链接转存完成！\n\
             ✅成功: {}个\n\
             ❌失败: {}个\n\
             🔄跳过重复文件: {}个\n\
             📁共 {} 个文件\n\
             📊成功转存体积: {:.2}GB\n\
             📊平均文件大小: {:.2}GB\n\
             ⏱️耗时: {:.2} 秒",
            self.success,
            self.failed,
            self.skipped,
            self.total,
            total_size_gb,
            avg_size_gb,
            self.cost.as_secs_f64()
        )
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct ResourceExportJson {
    #[serde(rename = "usesBase62EtagsInExport")]
    pub uses_base62_etags_in_export: bool,
    #[serde(rename = "etagEncrypted")]
    pub etag_encrypted: bool,
    #[serde(rename = "commonPath")]
    pub common_path: String,
    pub files: Vec<ResourceExportFile>,
}

#[derive(Debug, Deserialize)]
struct ResourceExportFile {
    pub path: String,
    pub etag: String,
    pub size: u64,
}
