use crate::{
    application::notify::{Message, MessageSender},
    error::AppResult,
    infrastructure::telegram::sender::TelegramBotSender,
};

#[derive(Clone)]
pub struct TelegramDeliveryContext {
    pub bot: teloxide::Bot,
    pub user_id: i64,
}

pub async fn on_send_telegram_message(
    ctx: TelegramDeliveryContext,
    payload: Message,
) -> AppResult<()> {
    TelegramBotSender::new(ctx.bot, ctx.user_id)
        .send(&payload)
        .await
}
