use crate::{application::notify::SendTelegramMessage, infrastructure::event_bus::Event};

pub mod publisher;
pub mod store;

impl Event for SendTelegramMessage {
    const NAME: &'static str = "SendTelegramMessage";
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn send_telegram_message_event_name_is_stable() {
        assert_eq!(SendTelegramMessage::NAME, "SendTelegramMessage");
    }
}
