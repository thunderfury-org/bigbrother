use teloxide::{
    prelude::Requester,
    sugar::request::RequestReplyExt,
    types::{ChatId, MessageId},
};

use crate::{
    application::notify::{SendTelegramMessage, TelegramMessageSender},
    error::{AppError, AppResult},
};

#[derive(Clone)]
pub struct TelegramBotSender {
    bot: teloxide::Bot,
    chat_id: ChatId,
}

impl TelegramBotSender {
    pub fn new(bot: teloxide::Bot, chat_id: i64) -> Self {
        Self {
            bot,
            chat_id: ChatId(chat_id),
        }
    }
}

impl TelegramMessageSender for TelegramBotSender {
    async fn send(&self, payload: &SendTelegramMessage) -> AppResult<()> {
        let result = match payload.reply_to {
            Some(reply_to) => {
                self.bot
                    .send_message(self.chat_id, payload.message.as_str())
                    .reply_to(MessageId(reply_to))
                    .await
            }
            None => {
                self.bot
                    .send_message(self.chat_id, payload.message.as_str())
                    .await
            }
        };

        result.map(|_| ()).map_err(|err| {
            AppError::Dependency(format!("failed to send telegram message: {}", err))
        })
    }
}
