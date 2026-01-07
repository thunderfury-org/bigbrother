use serde::{Deserialize, Serialize};

use crate::event_bus::Event;

#[derive(Serialize, Deserialize)]
pub struct SendTelegramMessage {
    pub message: String,
    pub reply_to: Option<i32>,
}

impl Event for SendTelegramMessage {
    const NAME: &'static str = "SendTelegramMessage";
}
