use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::json;

use super::RequestResult;

const API_HOST: &str = "https://open-api.123pan.com";
const PLATFORM_KEY: &str = "Platform";
const PLATFORM_VALUE: &str = "open_platform";

#[derive(Debug, Deserialize)]
struct CommonResponse<T> {
    code: i32,
    message: String,
    data: Option<T>,
}

#[derive(Debug, Deserialize)]
struct AccessToken {
    #[serde(rename = "accessToken")]
    token: String,
    #[serde(rename = "expiredAt", with = "time::serde::rfc3339")]
    expired_at: time::OffsetDateTime,
}

pub struct Client {
    client_id: String,
    client_secret: String,
}

impl Client {
    pub fn new(client_id: String, client_secret: String) -> Self {
        Self {
            client_id,
            client_secret,
        }
    }

    async fn post<P: Serialize, T: DeserializeOwned>(
        &self,
        path: String,
        query: Option<Vec<(&str, &str)>>,
        payload: Option<&P>,
    ) -> RequestResult<T> {
        let headers = Some(vec![(PLATFORM_KEY, PLATFORM_VALUE)]);
        super::http::post(format!("{API_HOST}{path}"), query, headers, payload).await
    }

    fn build_url(&self, path: &str) -> String {
        format!("{}{}", API_HOST, path)
    }

    async fn get_access_token(&self) -> RequestResult<String> {
        let resp: CommonResponse<AccessToken> = super::http::post(
            self.build_url("/api/v1/access_token"),
            None,
            Some(vec![(PLATFORM_KEY, PLATFORM_VALUE)]),
            Some(&json!({
                "clientID": self.client_id,
                "clientSecret": self.client_secret,
            })),
        )
        .await?;
        Ok(resp.data.unwrap().token)
    }
}
