use teloxide::prelude::*;
use tracing::{error, info};

use crate::{entity::keyword, state::AppState};

mod cmd;
mod format;
pub mod handler;
mod msg;

pub async fn run(state: AppState) {
    cmd::create_commands_in_background(state.bot());

    let handler = dptree::entry()
        .branch(Update::filter_channel_post().endpoint(handle_channel_post))
        .branch(
            Update::filter_message()
                .filter_command::<cmd::Command>()
                .endpoint(cmd::handle_command),
        )
        .branch(Update::filter_message().endpoint(handle_message));

    Dispatcher::builder(state.bot().clone(), handler)
        .enable_ctrlc_handler()
        .dependencies(dptree::deps![state])
        .build()
        .dispatch()
        .await;
}

async fn handle_channel_post(state: AppState, bot: Bot, msg: Message) -> ResponseResult<()> {
    let keywords = match keyword::list_all_keywords(state.db()).await {
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
            let processor = msg::MsgProcessor {
                state: &state,
                bot: &bot,
                msg: &msg,
                from_monitor: true,
            };
            return processor.process().await;
        }
    }

    if let Some(doc) = msg.document()
        && let Some(text) = doc.file_name.as_ref()
        && text.ends_with(".json")
    {
        for keyword in &filters {
            if text.contains(keyword) {
                let processor = msg::MsgProcessor {
                    state: &state,
                    bot: &bot,
                    msg: &msg,
                    from_monitor: true,
                };
                return processor.process().await;
            }
        }
    }

    Ok(())
}

async fn handle_message(state: AppState, bot: Bot, msg: Message) -> ResponseResult<()> {
    info!("Received message from {:?}", msg.chat);

    let user_id = UserId(state.config().get_telegram_config().user_id.try_into().unwrap());
    if msg.from.as_ref().is_none_or(|u| u.id != user_id) {
        info!("Ignoring message from unauthorized user: {:?}", msg.from);
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
