pub(crate) use crate::interface::telegram::{BotRuntime, run};

pub(crate) mod handler {
    pub(crate) use crate::interface::telegram::delivery::{
        TelegramDeliveryContext, on_send_telegram_message,
    };
}
