use serde::{Deserialize, Serialize};

use super::RequestResult;

const CHAT_COMPLETIONS_PATH: &str = "/chat/completions";

#[derive(Debug, Clone, Serialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Serialize)]
struct ChatCompletionRequest {
    model: String,
    messages: Vec<ChatMessage>,
}

#[derive(Debug, Deserialize)]
struct ChatCompletionResponse {
    choices: Vec<ChatChoice>,
}

#[derive(Debug, Deserialize)]
struct ChatChoice {
    message: ChatResponseMessage,
}

#[derive(Debug, Deserialize)]
struct ChatResponseMessage {
    content: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Client {
    api_key: String,
    base_url: String,
    model: String,
}

impl Client {
    pub fn new(api_key: &str, base_url: &str, model: &str) -> Self {
        Self {
            api_key: api_key.to_owned(),
            base_url: base_url.trim_end_matches('/').to_owned(),
            model: model.to_owned(),
        }
    }

    #[cfg(test)]
    fn with_host(api_key: &str, host: &str, model: &str) -> Self {
        Self {
            api_key: api_key.to_owned(),
            base_url: host.trim_end_matches('/').to_owned(),
            model: model.to_owned(),
        }
    }

    pub async fn chat_completion(
        &self,
        messages: Vec<ChatMessage>,
    ) -> RequestResult<Option<String>> {
        let url = format!("{}{}", self.base_url, CHAT_COMPLETIONS_PATH);
        let auth = format!("Bearer {}", self.api_key);
        let headers = [("Authorization", auth.as_str())];

        let payload = ChatCompletionRequest {
            model: self.model.clone(),
            messages,
        };

        let response: ChatCompletionResponse =
            super::http::post(url, None, Some(headers.to_vec()), Some(&payload)).await?;

        Ok(response
            .choices
            .into_iter()
            .next()
            .and_then(|c| c.message.content))
    }
}

#[cfg(test)]
mod tests {
    use super::super::RequestError;
    use super::*;
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{header, method, path},
    };

    fn client(server: &MockServer) -> Client {
        Client::with_host("test-key", server.uri().as_str(), "gpt-4.1-mini")
    }

    #[tokio::test]
    async fn chat_completion_sends_auth_and_model_returns_content() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .and(header("Authorization", "Bearer test-key"))
            .and(header("content-type", "application/json"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "choices": [{
                    "message": {
                        "role": "assistant",
                        "content": "{\"title\": \"民调局异闻录\", \"year\": \"2024\", \"language\": \"zh\"}"
                    }
                }]
            })))
            .mount(&server)
            .await;

        let result = client(&server)
            .chat_completion(vec![ChatMessage {
                role: "user".to_string(),
                content: "民调局异闻录第三季".to_string(),
            }])
            .await
            .unwrap();

        assert_eq!(
            result.unwrap(),
            "{\"title\": \"民调局异闻录\", \"year\": \"2024\", \"language\": \"zh\"}"
        );
    }

    #[tokio::test]
    async fn chat_completion_returns_none_when_content_is_null() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "choices": [{
                    "message": {
                        "role": "assistant",
                        "content": null
                    }
                }]
            })))
            .mount(&server)
            .await;

        let result = client(&server)
            .chat_completion(vec![ChatMessage {
                role: "user".to_string(),
                content: "some text".to_string(),
            }])
            .await
            .unwrap();

        assert!(result.is_none());
    }

    #[tokio::test]
    async fn chat_completion_returns_error_on_unauthorized() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(401))
            .mount(&server)
            .await;

        let result = client(&server)
            .chat_completion(vec![ChatMessage {
                role: "user".to_string(),
                content: "test".to_string(),
            }])
            .await;

        assert!(matches!(result, Err(RequestError::Unauthorized)));
    }

    #[tokio::test]
    async fn chat_completion_returns_error_on_rate_limit() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(429))
            .mount(&server)
            .await;

        let result = client(&server)
            .chat_completion(vec![ChatMessage {
                role: "user".to_string(),
                content: "test".to_string(),
            }])
            .await;

        assert!(matches!(result, Err(RequestError::TooManyRequests)));
    }
}
