use serde::Deserialize;
use teloxide::{net::Download, prelude::*, sugar::request::RequestReplyExt};

use super::client::RequestError;
use super::state::AppState;

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

async fn handle_channel_post(msg: Message) -> ResponseResult<()> {
    Ok(())
}

async fn handle_message(state: AppState, bot: Bot, msg: Message) -> ResponseResult<()> {
    if let Some(doc) = msg.document() {
        if !doc.file_name.as_ref().is_some_and(|n| n.ends_with(".json")) {
            bot.send_message(msg.chat.id, "不是 json 文件，忽略")
                .reply_to(msg.id)
                .await?;
            return Ok(());
        }

        bot.send_message(msg.chat.id, "开始处理").reply_to(msg.id).await?;
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
                            bot.send_message(msg.chat.id, format!("上传文件 {} 失败: {}", path, e))
                                .reply_to(msg.id)
                                .await?;
                        }
                    }
                }

                result.cost = start.elapsed();
                bot.send_message(msg.chat.id, result.to_string())
                    .reply_to(msg.id)
                    .await?;
            }
            Err(e) => {
                bot.send_message(msg.chat.id, format!("json 解析错误: {}", e))
                    .reply_to(msg.id)
                    .await?;
            }
        }
    }
    Ok(())
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
