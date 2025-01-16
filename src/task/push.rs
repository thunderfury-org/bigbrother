use lazy_static::lazy_static;
use serde::Deserialize;
use serde_json::json;
use tokio::sync::Mutex;
use tracing::{error, info};

use crate::common::error::{Error, Result};
use crate::common::state::AppState;

#[derive(Debug, Deserialize)]
struct AccessToken {
    errcode: i32,
    #[serde(default)]
    errmsg: String,
    access_token: String,
    expires_in: u64,
    #[serde(default)]
    expires_at: u64,
}

lazy_static! {
    static ref ACCESS_TOKEN_CACHE: Mutex<Option<AccessToken>> = Mutex::new(None);
}

pub async fn send(state: &AppState, message: &str) {
    let msg = format!("BigBrother 来信\n{}", message);
    if let Err(e) = send_inner(state, msg.as_str()).await {
        error!("send message failed, {}", e);
    }
}

async fn send_inner(state: &AppState, message: &str) -> Result<()> {
    let push = &state.config.get_app_config().push;
    let corp_id = push.params.get("corp_id").map_or("", String::as_ref);
    let agent_id = push.params.get("agent_id").map_or("", String::as_ref);
    let corp_secret = push.params.get("corp_secret").map_or("", String::as_ref);
    let user_id = push.params.get("user_id").map_or("@all", String::as_ref);

    if corp_id.is_empty() || agent_id.is_empty() || corp_secret.is_empty() {
        info!("corp id/agent id/corp secret is empty, skip send message");
        return Ok(());
    }

    let access_token = get_access_token(state, corp_id, corp_secret, false).await?;

    let url = format!(
        "https://qyapi.weixin.qq.com/cgi-bin/message/send?access_token={}",
        access_token
    );

    let result = state
        .http_client
        .post(url)
        .json(&json!({
            "touser": user_id,
            "agentid": agent_id,
            "msgtype": "text",
            "text": {
                "content": message
            }
        }))
        .send()
        .await;
    if result.is_err() {
        return Err(Error::Internal(format!(
            "send message to wecom failed, {}",
            result.err().unwrap()
        )));
    }

    let resp = result.unwrap();
    if !resp.status().is_success() {
        return Err(Error::Internal(format!(
            "send message to wecom failed, {}",
            resp.text()
                .await
                .unwrap_or_else(|_| "get response text error".to_string()),
        )));
    }

    let content = resp.text().await?;
    if !content.contains("\"errcode\":0") {
        return Err(Error::Internal(format!("send message to wecom failed, {}", content)));
    }
    return Ok(());
}

async fn get_access_token(state: &AppState, corp_id: &str, corp_secret: &str, refresh: bool) -> Result<String> {
    let mut cache = ACCESS_TOKEN_CACHE.lock().await;
    if !refresh && cache.is_some() {
        let token = cache.as_ref().unwrap();
        if token.expires_at > now() {
            return Ok(token.access_token.clone());
        }
    }

    info!("get wecom access token");
    let access_token = get_access_token_inner(state, corp_id, corp_secret).await?;
    *cache = Some(access_token);

    Ok(cache.as_ref().unwrap().access_token.clone())
}

async fn get_access_token_inner(state: &AppState, corp_id: &str, corp_secret: &str) -> Result<AccessToken> {
    let url = format!(
        "https://qyapi.weixin.qq.com/cgi-bin/gettoken?corpid={}&corpsecret={}",
        corp_id, corp_secret
    );

    let result = state.http_client.get(url).send().await;
    if result.is_err() {
        return Err(Error::Internal(format!(
            "get wecom access token failed, {}",
            result.err().unwrap()
        )));
    }

    let resp = result.unwrap();
    if !resp.status().is_success() {
        return Err(Error::Internal(format!(
            "get wecom access token failed, {}",
            resp.text()
                .await
                .unwrap_or_else(|_| "get response text error".to_string()),
        )));
    }

    let mut token: AccessToken = resp.json().await?;
    if token.errcode != 0 {
        return Err(Error::Internal(format!(
            "get wecom access token failed, errcode: {}, errmsg: {}",
            token.errcode, token.errmsg
        )));
    }
    // 有效期减少 10s
    token.expires_at = token.expires_in + now() - 10;

    Ok(token)
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
}
