use serde::Deserialize;
use serde_json::Value;

use super::{RequestError, RequestResult, http};

const DEFAULT_REPLY_MESSAGE: &str = "感谢分享,太棒了！";

#[derive(Debug, Clone)]
pub struct Client {
    base_url: String,
    cookie: String,
    reply_message: String,
}

impl Client {
    pub fn new(base_url: &str, cookie: &str, reply_message: &str) -> Self {
        let reply_message = reply_message.trim();
        Self {
            base_url: base_url.trim_end_matches('/').to_owned(),
            cookie: cookie.trim().to_owned(),
            reply_message: if reply_message.is_empty() {
                DEFAULT_REPLY_MESSAGE.to_owned()
            } else {
                reply_message.to_owned()
            },
        }
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    pub fn has_cookie(&self) -> bool {
        !self.cookie.is_empty()
    }

    pub async fn get_html(&self, url: &str) -> RequestResult<String> {
        http::get_text(url, Some(self.headers())).await
    }

    pub async fn reply(&self, tid: i64) -> RequestResult<()> {
        if !self.has_cookie() {
            return Err(RequestError::Unauthorized);
        }
        let url = format!("{}/?post-create-{tid}-1.htm", self.base_url);
        let referer = format!("{}/?thread-{tid}.htm", self.base_url);
        let mut headers = self.headers();
        headers.push(("x-requested-with", "XMLHttpRequest"));
        headers.push(("referer", referer.as_str()));
        headers.push(("origin", self.base_url.as_str()));
        let body = http::post_form(
            url,
            &[
                ("doctype", "1"),
                ("return_html", "1"),
                ("quotepid", "0"),
                ("message", self.reply_message.as_str()),
            ],
            Some(headers),
        )
        .await?;
        parse_xn_response(&body)
    }

    fn headers(&self) -> Vec<(&str, &str)> {
        let mut headers = Vec::new();
        if !self.cookie.is_empty() {
            headers.push(("cookie", self.cookie.as_str()));
        }
        headers
    }
}

fn parse_xn_response(body: &str) -> RequestResult<()> {
    let trimmed = body.trim();
    if trimmed.is_empty() {
        return Err(RequestError::Other("empty pan1 reply response".into()));
    }
    let value: XnResponse =
        serde_json::from_str(trimmed).map_err(|_| RequestError::Unauthorized)?;
    if value.is_success() {
        Ok(())
    } else {
        Err(RequestError::BadRequest(value.message_text()))
    }
}

#[derive(Debug, Deserialize)]
struct XnResponse {
    code: Value,
    #[serde(default)]
    message: Value,
}

impl XnResponse {
    fn is_success(&self) -> bool {
        match &self.code {
            Value::Number(n) => n.as_i64() == Some(0),
            Value::String(s) => s == "0",
            _ => false,
        }
    }

    fn message_text(&self) -> String {
        match &self.message {
            Value::String(s) => s.clone(),
            other => other.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{header, method, path},
    };

    #[tokio::test]
    async fn reply_posts_form_with_cookie() {
        let mock_server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/"))
            .and(header("cookie", "bbs_sid=abc"))
            .and(header("x-requested-with", "XMLHttpRequest"))
            .respond_with(
                ResponseTemplate::new(200).set_body_string(r#"{"code":0,"message":"ok"}"#),
            )
            .mount(&mock_server)
            .await;

        let client = Client::new(&mock_server.uri(), "bbs_sid=abc", "");
        client.reply(50570).await.unwrap();
    }

    #[tokio::test]
    async fn reply_without_cookie_is_unauthorized() {
        let client = Client::new("https://pan1.me", "", "");
        let err = client.reply(1).await.unwrap_err();
        assert!(matches!(err, RequestError::Unauthorized));
    }

    #[test]
    fn parse_xn_response_accepts_zero() {
        parse_xn_response(r#"{"code":0,"message":"ok"}"#).unwrap();
        parse_xn_response(r#"{"code":"0","message":"ok"}"#).unwrap();
        assert!(parse_xn_response(r#"{"code":-1,"message":"need login"}"#).is_err());
    }
}
