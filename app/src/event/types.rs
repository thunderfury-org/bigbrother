use serde::{Deserialize, Serialize};

/// 发送消息 Payload
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SendMessagePayload {
    pub chat_id: i64,
    pub text: String,
    pub reply_to_message_id: Option<i32>,
}

/// Channel 消息处理 Payload
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessChannelPostPayload {
    pub channel_id: i64,
    pub message_id: i32,
    pub message: serde_json::Value,
}
