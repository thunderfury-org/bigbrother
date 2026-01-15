use teloxide::{
    prelude::Requester,
    sugar::request::RequestReplyExt,
    types::{ChatId, MessageId},
};

use crate::{
    error::{AppError, AppResult},
    event::SendTelegramMessage,
    state::AppState,
};

pub async fn on_send_telegram_message(state: AppState, payload: SendTelegramMessage) -> AppResult<()> {
    let chat_id = ChatId(state.config().get_telegram_config().user_id);

    let res = match payload.reply_to {
        Some(reply_to) => {
            state
                .bot()
                .send_message(chat_id, payload.message.as_str())
                .reply_to(MessageId(reply_to))
                .await
        }
        None => state.bot().send_message(chat_id, payload.message.as_str()).await,
    };

    match res {
        Ok(_) => Ok(()),
        Err(e) => Err(AppError::Internal(format!("failed to send telegram message: {}", e))),
    }
}
