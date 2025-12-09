use teloxide::prelude::*;

use crate::state::AppState;

mod msg;

pub async fn run(state: AppState) {
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
    let filters = match &state.config.get_telegram_config().filters {
        Some(f) => f,
        None => return Ok(()),
    };

    let chat_id = ChatId(state.config.get_telegram_config().user_id);

    let text = msg.text().or(msg.caption()).unwrap_or_default();
    for keyword in filters {
        if text.contains(keyword) {
            let m = bot.forward_message(chat_id, msg.chat.id, msg.id).await?;
            let processor = msg::MsgProcessor {
                state: &state,
                bot: &bot,
                msg: &m,
            };
            return processor.process().await;
        }
    }

    if let Some(doc) = msg.document()
        && let Some(text) = doc.file_name.as_ref()
        && text.ends_with(".json")
    {
        for keyword in filters {
            if text.contains(keyword) {
                let m = bot.forward_message(chat_id, msg.chat.id, msg.id).await?;
                let processor = msg::MsgProcessor {
                    state: &state,
                    bot: &bot,
                    msg: &m,
                };
                return processor.process().await;
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
    };
    processor.process().await
}
