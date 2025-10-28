use teloxide::prelude::*;

use crate::state::AppState;

mod msg;

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

async fn handle_channel_post(state: &AppState, bot: &Bot, msg: &Message) -> ResponseResult<()> {
    if msg
        .caption()
        .is_some_and(|c| c.contains("天地剑心") || c.contains("红石榴餐厅") || c.contains("暗河传"))
    {
        handle_message(state, bot, msg).await?;
    }
    Ok(())
}

async fn handle_message(state: &AppState, bot: &Bot, msg: &Message) -> ResponseResult<()> {
    Ok(())
}
