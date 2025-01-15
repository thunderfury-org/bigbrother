use serde_json::json;
use tracing::{error, info};

use crate::common::state::AppState;

pub async fn send(state: &AppState, message: &str) {
    let push = &state.config.get_app_config().push;
    let corp_id = push.params.get("corp_id").unwrap_or(&"".to_string());
    let agent_id = push.params.get("agent_id").unwrap_or(&"".to_string());
    let corp_secret = push.params.get("corp_secret").unwrap_or(&"".to_string());
    let user_id = push.params.get("user_id").unwrap_or(&"@all".to_string());

    if corp_id.is_empty() || agent_id.is_empty() || corp_secret.is_empty() {
        info!("corp id/agent id/corp secret is empty, skip send message");
        return;
    }

    let url = format!(
        "https://qyapi.weixin.qq.com/cgi-bin/message/send?access_token={}",
        corp_secret
    );

    let result = state
        .http_client
        .post(url)
        .json(&json!({"touser": user_id,
            "agentid": agent_id,
            "msgtype": "text",
            "text": {
                "content": message
            },
            "duplicate_check_interval": 600}))
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
