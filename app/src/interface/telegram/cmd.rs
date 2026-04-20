use teloxide::{
    prelude::*,
    sugar::request::RequestReplyExt,
    types::{CallbackQuery, InlineKeyboardButton, InlineKeyboardMarkup, MenuButton},
    utils::command::BotCommands,
};
use tracing::{error, info};

use super::BotRuntime;

const DELETE_KEYWORD_PREFIX: &str = "delete_keyword:";
const DELETE_KEYWORD_CANCEL: &str = "delete_keyword:cancel";
const DELETE_MEDIA_SELECT_PREFIX: &str = "delete_media_select:";
const DELETE_MEDIA_CONFIRM_PREFIX: &str = "delete_media_confirm:";
const DELETE_MEDIA_CANCEL_PREFIX: &str = "delete_media_cancel:";

#[derive(BotCommands, Clone)]
#[command(rename_rule = "snake_case", description = "BigBrother 可用命令:")]
pub(super) enum Command {
    #[command(description = "查看帮助")]
    Help,
    #[command(description = "查看所有关键字")]
    ListKeywords,
    #[command(description = "添加新的关键字")]
    AddKeyword(String),
    #[command(description = "交互式删除关键字")]
    DeleteKeyword,
    #[command(description = "搜索并删除媒体目录")]
    DeleteMedia(String),
    #[command(description = "同步远程库到本地")]
    SyncStrm,
}

pub(super) fn create_commands_in_background(bot: &Bot) {
    let bot_clone = bot.clone();
    tokio::spawn(async move {
        while let Err(e) = create_commands(&bot_clone).await {
            error!(
                "Failed to create telegram bot commands, will retry later: {}",
                e
            );
            tokio::time::sleep(std::time::Duration::from_secs(60)).await;
        }
        info!("Telegram bot commands created successfully");
    });
}

async fn create_commands(bot: &Bot) -> ResponseResult<()> {
    bot.set_my_commands(Command::bot_commands()).await?;
    bot.set_chat_menu_button()
        .menu_button(MenuButton::Commands)
        .await?;
    Ok(())
}

pub(super) async fn handle_command(
    runtime: BotRuntime,
    bot: Bot,
    msg: Message,
    cmd: Command,
) -> ResponseResult<()> {
    if msg.from.as_ref().is_none_or(|u| u.id != runtime.user_id) {
        // Ignore messages not from the specified user
        return Ok(());
    }

    match cmd {
        Command::Help => {
            bot.send_message(msg.chat.id, Command::descriptions().to_string())
                .await?;
        }
        Command::ListKeywords => list_keywords(&runtime, &bot, &msg).await?,
        Command::AddKeyword(keyword) => add_keyword(&runtime, &bot, &msg, &keyword).await?,
        Command::DeleteKeyword => prompt_delete_keyword(&runtime, &bot, &msg).await?,
        Command::DeleteMedia(keyword) => delete_media_cmd(&runtime, &bot, &msg, &keyword).await?,
        Command::SyncStrm => sync_strm_cmd(&runtime, &bot, &msg).await?,
    }
    Ok(())
}

pub(super) async fn handle_callback_query(
    runtime: BotRuntime,
    bot: Bot,
    query: CallbackQuery,
) -> ResponseResult<()> {
    if query.from.id != runtime.user_id {
        bot.answer_callback_query(query.id.clone())
            .text("未授权")
            .await?;
        return Ok(());
    }

    let Some(data) = query.data.as_deref() else {
        return Ok(());
    };

    if data.starts_with(DELETE_KEYWORD_PREFIX) {
        return handle_delete_keyword_callback(runtime, bot, query).await;
    }
    if data.starts_with(DELETE_MEDIA_SELECT_PREFIX)
        || data.starts_with(DELETE_MEDIA_CONFIRM_PREFIX)
        || data.starts_with(DELETE_MEDIA_CANCEL_PREFIX)
    {
        return handle_delete_media_callback(runtime, bot, query).await;
    }

    Ok(())
}

pub(super) fn delete_media_usage() -> &'static str {
    "用法: /delete_media <关键词>"
}

pub(super) fn is_delete_media_usage_request(msg: &Message) -> bool {
    msg.text()
        .map(|text| {
            let command = text.split_whitespace().next().unwrap_or_default();
            command == "/delete_media" || command.starts_with("/delete_media@")
        })
        .unwrap_or(false)
}

async fn list_keywords(runtime: &BotRuntime, bot: &Bot, msg: &Message) -> ResponseResult<()> {
    let keywords = match runtime.keyword_service().list().await {
        Ok(ks) => ks,
        Err(e) => {
            error!("Failed to list keywords: {}", e);
            bot.send_message(msg.chat.id, "查询关键字失败")
                .reply_to(msg.id)
                .await?;
            return Ok(());
        }
    };

    if keywords.is_empty() {
        bot.send_message(msg.chat.id, "没有关键字")
            .reply_to(msg.id)
            .await?;
    } else {
        let keyword_list = keywords
            .iter()
            .map(|k| k.value.as_str())
            .collect::<Vec<&str>>()
            .join("\n");
        bot.send_message(msg.chat.id, format!("关键字:\n\n{}", keyword_list))
            .reply_to(msg.id)
            .await?;
    }
    Ok(())
}

async fn add_keyword(
    runtime: &BotRuntime,
    bot: &Bot,
    msg: &Message,
    keyword: &str,
) -> ResponseResult<()> {
    let kw = keyword.trim();
    if kw.is_empty() {
        bot.send_message(msg.chat.id, "关键字不能为空")
            .reply_to(msg.id)
            .await?;
        return Ok(());
    }

    match runtime.keyword_service().add(kw).await {
        Ok(keyword) => {
            bot.send_message(msg.chat.id, format!("关键字 '{}' 添加成功", keyword))
                .reply_to(msg.id)
                .await?;
        }
        Err(e) => {
            error!("Failed to add keyword '{}': {}", kw, e);
            bot.send_message(msg.chat.id, format!("添加关键字 '{}' 失败", kw))
                .reply_to(msg.id)
                .await?;
        }
    }
    Ok(())
}

async fn prompt_delete_keyword(
    runtime: &BotRuntime,
    bot: &Bot,
    msg: &Message,
) -> ResponseResult<()> {
    let keywords = match runtime.keyword_service().list().await {
        Ok(ks) => ks,
        Err(e) => {
            error!("Failed to list keywords for delete: {}", e);
            bot.send_message(msg.chat.id, "查询关键字失败")
                .reply_to(msg.id)
                .await?;
            return Ok(());
        }
    };

    if keywords.is_empty() {
        bot.send_message(msg.chat.id, "没有关键字可删除")
            .reply_to(msg.id)
            .await?;
        return Ok(());
    }

    let mut keyboard: Vec<Vec<InlineKeyboardButton>> = keywords
        .into_iter()
        .map(|keyword| {
            vec![InlineKeyboardButton::callback(
                keyword.value,
                format!("{}{}", DELETE_KEYWORD_PREFIX, keyword.id),
            )]
        })
        .collect();
    keyboard.push(vec![InlineKeyboardButton::callback(
        "取消",
        DELETE_KEYWORD_CANCEL,
    )]);

    bot.send_message(msg.chat.id, "请选择要删除的关键字")
        .reply_to(msg.id)
        .reply_markup(InlineKeyboardMarkup::new(keyboard))
        .await?;

    Ok(())
}

async fn delete_media_cmd(
    runtime: &BotRuntime,
    bot: &Bot,
    msg: &Message,
    keyword: &str,
) -> ResponseResult<()> {
    let keyword = keyword.trim();
    if keyword.is_empty() {
        bot.send_message(msg.chat.id, delete_media_usage())
            .reply_to(msg.id)
            .await?;
        return Ok(());
    }

    let candidates = match runtime
        .delete_media_service()
        .search_candidates(keyword)
        .await
    {
        Ok(candidates) => candidates,
        Err(e) => {
            error!("Failed to search media candidates '{}': {}", keyword, e);
            bot.send_message(msg.chat.id, "搜索媒体目录失败")
                .reply_to(msg.id)
                .await?;
            return Ok(());
        }
    };

    if candidates.is_empty() {
        bot.send_message(msg.chat.id, "未找到匹配的媒体目录")
            .reply_to(msg.id)
            .await?;
        return Ok(());
    }

    runtime.cache_delete_media_candidates(&candidates).await;

    let text = candidates
        .iter()
        .enumerate()
        .map(|(index, candidate)| format!("{}. /{}", index + 1, candidate.relative_path))
        .collect::<Vec<_>>()
        .join("\n\n");
    let keyboard = candidates
        .iter()
        .map(|candidate| {
            vec![InlineKeyboardButton::callback(
                format!("/{}", candidate.relative_path),
                format!("{}{id}", DELETE_MEDIA_SELECT_PREFIX, id = candidate.dir_id),
            )]
        })
        .collect::<Vec<_>>();

    bot.send_message(msg.chat.id, format!("请选择要删除的媒体目录:\n\n{text}"))
        .reply_to(msg.id)
        .reply_markup(InlineKeyboardMarkup::new(keyboard))
        .await?;

    Ok(())
}

async fn handle_delete_keyword_callback(
    runtime: BotRuntime,
    bot: Bot,
    query: CallbackQuery,
) -> ResponseResult<()> {
    let Some(data) = query.data.as_deref() else {
        return Ok(());
    };

    if data == DELETE_KEYWORD_CANCEL {
        bot.answer_callback_query(query.id.clone())
            .text("已取消")
            .await?;
        if let Some(message) = query.regular_message() {
            bot.edit_message_text(message.chat.id, message.id, "已取消删除关键字")
                .reply_markup(InlineKeyboardMarkup::default())
                .await?;
        }
        return Ok(());
    }

    let Some(keyword_id) = data
        .strip_prefix(DELETE_KEYWORD_PREFIX)
        .and_then(|v| v.parse::<i64>().ok())
    else {
        bot.answer_callback_query(query.id.clone())
            .text("无效的关键字")
            .await?;
        return Ok(());
    };

    let result_text = match runtime.keyword_service().delete(keyword_id).await {
        Ok(_) => "关键字删除成功",
        Err(e) => {
            error!("Failed to delete keyword by id '{}': {}", keyword_id, e);
            "删除关键字失败"
        }
    };

    bot.answer_callback_query(query.id.clone())
        .text(result_text)
        .await?;
    if let Some(message) = query.regular_message() {
        bot.edit_message_text(message.chat.id, message.id, result_text)
            .reply_markup(InlineKeyboardMarkup::default())
            .await?;
    }

    Ok(())
}

async fn handle_delete_media_callback(
    runtime: BotRuntime,
    bot: Bot,
    query: CallbackQuery,
) -> ResponseResult<()> {
    let Some(data) = query.data.as_deref() else {
        return Ok(());
    };

    if let Some(dir_id) = parse_callback_dir_id(data, DELETE_MEDIA_CANCEL_PREFIX) {
        runtime.remove_delete_media_candidate(dir_id).await;
        bot.answer_callback_query(query.id.clone())
            .text("已取消")
            .await?;
        if let Some(message) = query.regular_message() {
            bot.edit_message_text(message.chat.id, message.id, "已取消删除媒体目录")
                .reply_markup(InlineKeyboardMarkup::default())
                .await?;
        }
        return Ok(());
    }

    let Some((dir_id, is_confirm)) = parse_delete_media_action(data) else {
        bot.answer_callback_query(query.id.clone())
            .text("无效的媒体目录")
            .await?;
        return Ok(());
    };

    let Some(candidate) = runtime.get_delete_media_candidate(dir_id).await else {
        bot.answer_callback_query(query.id.clone())
            .text("目标已失效")
            .await?;
        if let Some(message) = query.regular_message() {
            bot.edit_message_text(message.chat.id, message.id, "删除目标已失效，请重新搜索")
                .reply_markup(InlineKeyboardMarkup::default())
                .await?;
        }
        return Ok(());
    };

    if !is_confirm {
        bot.answer_callback_query(query.id.clone())
            .text("请确认删除")
            .await?;
        if let Some(message) = query.regular_message() {
            bot.edit_message_text(
                message.chat.id,
                message.id,
                format!("确认删除以下媒体目录？\n\n/{}", candidate.relative_path),
            )
            .reply_markup(InlineKeyboardMarkup::new(vec![
                vec![InlineKeyboardButton::callback(
                    "确认删除",
                    format!("{}{dir_id}", DELETE_MEDIA_CONFIRM_PREFIX),
                )],
                vec![InlineKeyboardButton::callback(
                    "取消",
                    format!("{}{dir_id}", DELETE_MEDIA_CANCEL_PREFIX),
                )],
            ]))
            .await?;
        }
        return Ok(());
    }

    runtime.remove_delete_media_candidate(dir_id).await;
    let result_text = match runtime
        .delete_media_service()
        .delete_candidate(&candidate)
        .await
    {
        Ok(()) => "媒体目录删除成功",
        Err(e) => {
            error!("Failed to delete media dir '{}': {}", dir_id, e);
            "删除媒体目录失败"
        }
    };

    bot.answer_callback_query(query.id.clone())
        .text(result_text)
        .await?;
    if let Some(message) = query.regular_message() {
        bot.edit_message_text(
            message.chat.id,
            message.id,
            format!("{}\n\n/{}", result_text, candidate.relative_path),
        )
        .reply_markup(InlineKeyboardMarkup::default())
        .await?;
    }

    Ok(())
}

fn parse_delete_media_action(data: &str) -> Option<(i64, bool)> {
    if let Some(dir_id) = parse_callback_dir_id(data, DELETE_MEDIA_SELECT_PREFIX) {
        return Some((dir_id, false));
    }
    parse_callback_dir_id(data, DELETE_MEDIA_CONFIRM_PREFIX).map(|dir_id| (dir_id, true))
}

fn parse_callback_dir_id(data: &str, prefix: &str) -> Option<i64> {
    data.strip_prefix(prefix)
        .and_then(|v| v.parse::<i64>().ok())
}

async fn sync_strm_cmd(runtime: &BotRuntime, bot: &Bot, msg: &Message) -> ResponseResult<()> {
    info!("Starting strm sync");
    bot.send_message(msg.chat.id, "开始同步远程库，请稍候...")
        .reply_to(msg.id)
        .await?;
    match runtime.sync_service().execute().await {
        Ok(()) => {
            info!("Strm sync completed successfully");
            bot.send_message(msg.chat.id, "同步完成")
                .reply_to(msg.id)
                .await?;
        }
        Err(e) => {
            error!("Failed to sync strm: {}", e);
            bot.send_message(msg.chat.id, "同步失败")
                .reply_to(msg.id)
                .await?;
        }
    }
    Ok(())
}
