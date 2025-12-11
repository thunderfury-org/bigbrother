use teloxide::{prelude::*, sugar::request::RequestReplyExt, types::MenuButton, utils::command::BotCommands};
use tracing::{error, info};

use crate::{entity, state::AppState};

#[derive(BotCommands, Clone)]
#[command(rename_rule = "snake_case", description = "BigBrother 可用命令:")]
pub(super) enum Command {
    #[command(description = "查看帮助")]
    Help,
    #[command(description = "查看所有关键字")]
    ListKeywords,
    #[command(description = "添加新的关键字")]
    AddKeyword(String),
    #[command(description = "删除指定的关键字")]
    DeleteKeyword(String),
}

pub(super) fn create_commands_in_background(bot: &Bot) {
    let bot_clone = bot.clone();
    tokio::spawn(async move {
        while let Err(e) = create_commands(&bot_clone).await {
            error!("Failed to create telegram bot commands, will retry later: {}", e);
            tokio::time::sleep(std::time::Duration::from_secs(60)).await;
        }
        info!("Telegram bot commands created successfully");
    });
}

async fn create_commands(bot: &Bot) -> ResponseResult<()> {
    bot.set_my_commands(Command::bot_commands()).await?;
    bot.set_chat_menu_button().menu_button(MenuButton::Commands).await?;
    Ok(())
}

pub(super) async fn handle_command(state: AppState, bot: Bot, msg: Message, cmd: Command) -> ResponseResult<()> {
    let user_id = UserId(state.config.get_telegram_config().user_id.try_into().unwrap());
    if msg.from.as_ref().is_none_or(|u| u.id != user_id) {
        // Ignore messages not from the specified user
        return Ok(());
    }

    match cmd {
        Command::Help => {
            bot.send_message(msg.chat.id, Command::descriptions().to_string())
                .await?;
        }
        Command::ListKeywords => list_keywords(&state, &bot, &msg).await?,
        Command::AddKeyword(keyword) => add_keyword(&state, &bot, &msg, &keyword).await?,
        Command::DeleteKeyword(keyword) => delete_keyword(&state, &bot, &msg, &keyword).await?,
    }
    Ok(())
}

async fn list_keywords(state: &AppState, bot: &Bot, msg: &Message) -> ResponseResult<()> {
    let keywords = match entity::keyword::list_all_keywords(&state.db).await {
        Ok(ks) => ks,
        Err(e) => {
            error!("Failed to list keywords: {}", e);
            bot.send_message(msg.chat.id, "查询关键字失败").reply_to(msg.id).await?;
            return Ok(());
        }
    };

    if keywords.is_empty() {
        bot.send_message(msg.chat.id, "没有关键字").reply_to(msg.id).await?;
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

async fn add_keyword(state: &AppState, bot: &Bot, msg: &Message, keyword: &str) -> ResponseResult<()> {
    let kw = keyword.trim();
    if kw.is_empty() {
        bot.send_message(msg.chat.id, "关键字不能为空").reply_to(msg.id).await?;
        return Ok(());
    }

    match entity::keyword::add_new_keyword(&state.db, kw).await {
        Ok(_) => {
            bot.send_message(msg.chat.id, format!("关键字 '{}' 添加成功", kw))
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

async fn delete_keyword(state: &AppState, bot: &Bot, msg: &Message, keyword: &str) -> ResponseResult<()> {
    match entity::keyword::get_keyword(&state.db, keyword).await {
        Ok(Some(kw)) => match entity::keyword::delete_keyword_by_id(&state.db, kw.id).await {
            Ok(_) => {
                bot.send_message(msg.chat.id, format!("关键字 '{}' 删除成功。", keyword))
                    .reply_to(msg.id)
                    .await?;
            }
            Err(e) => {
                error!("Failed to delete keyword '{}': {}", keyword, e);
                bot.send_message(msg.chat.id, format!("删除关键字 '{}' 失败", keyword))
                    .reply_to(msg.id)
                    .await?;
            }
        },
        Ok(None) => {
            bot.send_message(msg.chat.id, format!("关键字 '{}' 不存在。", keyword))
                .reply_to(msg.id)
                .await?;
        }
        Err(e) => {
            error!("Failed to query keyword '{}': {}", keyword, e);
            bot.send_message(msg.chat.id, "查询关键字失败").reply_to(msg.id).await?;
        }
    }

    Ok(())
}
