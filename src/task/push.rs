use serde::Deserialize;

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

pub async fn send(state: &AppState, message: &str) {
}
