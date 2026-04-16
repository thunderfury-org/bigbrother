use crate::{
    application::notify::{DeliverTelegramMessageService, SendTelegramMessage},
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
    payload: SendTelegramMessage,
) -> AppResult<()> {
    DeliverTelegramMessageService::new(TelegramBotSender::new(ctx.bot, ctx.user_id))
        .deliver(&payload)
        .await
}
