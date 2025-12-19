use std::sync::Arc;

use teloxide::prelude::*;
use teloxide::types::{MessageId, ReplyParameters};
use tracing::error;

use crate::{
    entity::keyword,
    event::types::{ProcessChannelPostPayload, SendMessagePayload},
    state::AppState,
};

mod cmd;
mod format;
mod msg;

pub async fn run(state: AppState) {
    let bot = Bot::new(state.config.get_telegram_config().bot_token.as_str());

    // 订阅 send_message 事件
    let bot_clone = bot.clone();
    state
        .event_bus
        .sub("send_message", move |payload: SendMessagePayload| {
            let bot = bot_clone.clone();
            async move {
                let mut request = bot.send_message(ChatId(payload.chat_id), payload.text);

                if let Some(reply_to) = payload.reply_to_message_id {
                    request = request.reply_parameters(ReplyParameters::new(MessageId(reply_to)));
                }

                request.await?;
                Ok(())
            }
        });

    // 订阅 process_channel_post 事件
    let state_clone = Arc::new(state.clone());
    let bot_clone = bot.clone();
    state
        .event_bus
        .sub("process_channel_post", move |payload: ProcessChannelPostPayload| {
            let state = state_clone.clone();
            let bot = bot_clone.clone();
            async move {
                // 反序列化 Telegram Message
                let message: teloxide::types::Message = serde_json::from_value(payload.message)?;

                // 复用现有的 MsgProcessor 逻辑
                let processor = msg::MsgProcessor {
                    state: &state,
                    bot: &bot,
                    msg: &message,
                    from_monitor: true,
                };

                processor.process().await?;
                Ok(())
            }
        });

    cmd::create_commands_in_background(&bot);

    let handler = dptree::entry()
        .branch(Update::filter_channel_post().endpoint(handle_channel_post))
        .branch(
            Update::filter_message()
                .filter_command::<cmd::Command>()
                .endpoint(cmd::handle_command),
        )
        .branch(Update::filter_message().endpoint(handle_message));

    Dispatcher::builder(bot, handler)
        .enable_ctrlc_handler()
        .dependencies(dptree::deps![state])
        .build()
        .dispatch()
        .await;
}

async fn handle_channel_post(state: AppState, _bot: Bot, msg: Message) -> ResponseResult<()> {
    let keywords = match keyword::list_all_keywords(&state.db).await {
        Ok(keywords) => keywords,
        Err(e) => {
            error!("Failed to query keywords from database: {e}");
            return Ok(());
        }
    };

    if keywords.is_empty() {
        return Ok(());
    }

    let filters: Vec<String> = keywords.into_iter().map(|k| k.value).collect();

    let text = msg.text().or(msg.caption()).unwrap_or_default();
    for keyword in &filters {
        if text.contains(keyword) {
            // 序列化消息并发布事件
            let message_json = match serde_json::to_value(&msg) {
                Ok(json) => json,
                Err(e) => {
                    error!("Failed to serialize message: {}", e);
                    return Ok(());
                }
            };

            let _ = state
                .event_bus
                .publish(
                    "process_channel_post",
                    ProcessChannelPostPayload {
                        channel_id: msg.chat.id.0,
                        message_id: msg.id.0,
                        message: message_json,
                    },
                )
                .await;

            return Ok(());
        }
    }

    if let Some(doc) = msg.document()
        && let Some(text) = doc.file_name.as_ref()
        && text.ends_with(".json")
    {
        for keyword in &filters {
            if text.contains(keyword) {
                // 序列化消息并发布事件
                let message_json = match serde_json::to_value(&msg) {
                    Ok(json) => json,
                    Err(e) => {
                        error!("Failed to serialize message: {}", e);
                        return Ok(());
                    }
                };

                let _ = state
                    .event_bus
                    .publish(
                        "process_channel_post",
                        ProcessChannelPostPayload {
                            channel_id: msg.chat.id.0,
                            message_id: msg.id.0,
                            message: message_json,
                        },
                    )
                    .await;

                return Ok(());
            }
        }
    }

    Ok(())
}

async fn handle_message(state: AppState, bot: Bot, msg: Message) -> ResponseResult<()> {
    let user_id = UserId(state.config.get_telegram_config().user_id.try_into().unwrap());
    if msg.from.as_ref().is_none_or(|u| u.id != user_id) {
        // Ignore messages not from the specified user
        return Ok(());
    }

    let processor = msg::MsgProcessor {
        state: &state,
        bot: &bot,
        msg: &msg,
        from_monitor: false,
    };
    processor.process().await
}
