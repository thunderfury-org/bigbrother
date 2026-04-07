use serde::{Deserialize, Serialize};

use crate::infrastructure::event_bus::Event;

pub mod publisher;
pub mod store;

#[derive(Serialize, Deserialize)]
pub struct SendTelegramMessage {
    pub message: String,
    pub reply_to: Option<i32>,
}

impl Event for SendTelegramMessage {
    const NAME: &'static str = "SendTelegramMessage";
}
