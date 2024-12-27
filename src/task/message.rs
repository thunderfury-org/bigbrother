use serde_json::json;
use tracing::{error, info};

use crate::common::state::AppState;

pub async fn send(state: &AppState, message: &str) {
    let token = state.config.get_app_config().telegram_bot_token.as_str();
    let chat_id = state.config.get_app_config().telegram_chat_id.as_str();
    if token.is_empty() || chat_id.is_empty() {
        info!("telegram bot token or chat id is empty, skip send message");
        return;
    }

    let url = format!("https://api.telegram.org/bot{}/sendMessage", token);

    let result = state
        .http_client
        .post(url)
        .json(&json!({"chat_id": chat_id, "text": message}))
        .send()
        .await;
    if result.is_err() {
        error!("send message to telegram failed, {}", result.err().unwrap());
        return;
    }

    let resp = result.unwrap();
    if !resp.status().is_success() {
        error!(
            "send message to telegram failed, {}",
            resp.text()
                .await
                .unwrap_or_else(|_| "get response text error".to_string()),
        );
    }
}
