use std::{path::Path, sync::LazyLock};

use reqwest::{IntoUrl, StatusCode};
use reqwest_middleware::{ClientBuilder, ClientWithMiddleware};
use reqwest_retry::{RetryTransientMiddleware, policies::ExponentialBackoff};
use serde::{Serialize, de::DeserializeOwned};

use super::{RequestError, RequestResult};

pub const UA_VALUE: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/141.0.0.0 Safari/537.36";
pub const AUTH_KEY: &str = "Authorization";

static HTTP_CLIENT: LazyLock<ClientWithMiddleware> = LazyLock::new(|| {
    let req_client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .expect("failed to create http client");

    let retry_policy = ExponentialBackoff::builder().build_with_max_retries(3);
    ClientBuilder::new(req_client)
        // Retry failed requests.
        .with(RetryTransientMiddleware::new_with_policy(retry_policy))
        .build()
});

pub async fn download_file(url: &str, path: &str) -> RequestResult<()> {
    let response = HTTP_CLIENT.get(url).send().await?;
    let status = response.status();
    let url = response.url().to_string();
    let payload = response.bytes().await?;

    if status.is_success() {
        // write payload to file
        tokio::fs::create_dir_all(Path::new(path).parent().unwrap())
            .await
            .map_err(|e| RequestError::Other(format!("create dir failed, {}", e)))?;
        tokio::fs::write(path, payload)
            .await
            .map_err(|e| RequestError::Other(format!("write file failed, {}", e)))?;
        return Ok(());
    }

    match status {
        StatusCode::NOT_FOUND => Err(RequestError::NotFound(url)),
        s if s.is_client_error() => Err(RequestError::BadRequest(format!(
            "http request to {url} failed, status: {status}, payload: {payload:?}",
        ))),
        _ => Err(RequestError::ServerError(format!(
            "http request to {url} failed, status: {status}, payload: {payload:?}",
        ))),
    }
}

pub async fn get<U: IntoUrl, T: DeserializeOwned>(
    url: U,
    query: Option<Vec<(&str, &str)>>,
    headers: Option<Vec<(&str, &str)>>,
) -> RequestResult<T> {
    let response = get_response(url, query, headers).await?;
    process_response(response).await
}

pub async fn get_response<U: IntoUrl>(
    url: U,
    query: Option<Vec<(&str, &str)>>,
    headers: Option<Vec<(&str, &str)>>,
) -> RequestResult<reqwest::Response> {
    let mut request = HTTP_CLIENT.get(url);
    if let Some(q) = query {
        request = request.query(&q);
    }
    for (k, v) in default_headers() {
        request = request.header(k, v);
    }
    if let Some(h) = headers {
        for (k, v) in h {
            request = request.header(k, v);
        }
    }

    request
        .send()
        .await
        .map_err(|e| RequestError::Other(format!("http get failed, {}", e)))
}

pub async fn post<U: IntoUrl, P: Serialize, T: DeserializeOwned>(
    url: U,
    query: Option<Vec<(&str, &str)>>,
    headers: Option<Vec<(&str, &str)>>,
    payload: Option<&P>,
) -> RequestResult<T> {
    let response = post_response(url, query, headers, payload).await?;
    process_response(response).await
}

pub async fn post_response<U: IntoUrl, P: Serialize>(
    url: U,
    query: Option<Vec<(&str, &str)>>,
    headers: Option<Vec<(&str, &str)>>,
    payload: Option<&P>,
) -> RequestResult<reqwest::Response> {
    let mut request = HTTP_CLIENT.post(url);
    if let Some(q) = query {
        request = request.query(&q);
    }
    for (k, v) in default_headers() {
        request = request.header(k, v);
    }
    if let Some(h) = headers {
        for (k, v) in h {
            request = request.header(k, v);
        }
    }
    if let Some(p) = payload {
        match serde_json::to_vec(p) {
            Ok(body) => {
                request = request.header("content-type", "application/json");
                request = request.body(body);
            }
            Err(err) => {
                return Err(RequestError::Other(format!(
                    "serialize http payload failed, {}",
                    err
                )));
            }
        }
    }

    request
        .send()
        .await
        .map_err(|e| RequestError::Other(format!("http post failed, {}", e)))
}

pub async fn get_text<U: IntoUrl>(
    url: U,
    headers: Option<Vec<(&str, &str)>>,
) -> RequestResult<String> {
    let mut request = HTTP_CLIENT.get(url);
    request = request.header("user-agent", UA_VALUE);
    request = request.header("accept", "text/html,application/xhtml+xml;q=0.9,*/*;q=0.8");
    if let Some(h) = headers {
        for (k, v) in h {
            request = request.header(k, v);
        }
    }
    let response = request
        .send()
        .await
        .map_err(|e| RequestError::Other(format!("http get failed, {}", e)))?;
    read_success_text(response).await
}

pub async fn post_form<U: IntoUrl>(
    url: U,
    form: &[(&str, &str)],
    headers: Option<Vec<(&str, &str)>>,
) -> RequestResult<String> {
    let mut request = HTTP_CLIENT.post(url).form(form);
    request = request.header("user-agent", UA_VALUE);
    request = request.header("accept", "*/*");
    if let Some(h) = headers {
        for (k, v) in h {
            request = request.header(k, v);
        }
    }
    let response = request
        .send()
        .await
        .map_err(|e| RequestError::Other(format!("http post failed, {}", e)))?;
    read_success_text(response).await
}

async fn read_success_text(response: reqwest::Response) -> RequestResult<String> {
    let status = response.status();
    let url = response.url().to_string();
    let payload = response.text().await?;
    if status.is_success() {
        Ok(payload)
    } else {
        Err(status_to_error(status, &url, &payload))
    }
}

async fn process_response<T: DeserializeOwned>(response: reqwest::Response) -> RequestResult<T> {
    let status = response.status();
    let url = response.url().to_string();
    let payload = response.text().await?;

    if status.is_success() {
        return match serde_json::from_str::<T>(&payload) {
            Ok(data) => Ok(data),
            Err(e) => Err(RequestError::Other(format!(
                "http request to {url} failed, decode payload failed, {e}, payload: {payload}",
            ))),
        };
    }

    Err(status_to_error(status, &url, &payload))
}

fn status_to_error(status: StatusCode, url: &str, payload: &str) -> RequestError {
    match status {
        StatusCode::UNAUTHORIZED => RequestError::Unauthorized,
        StatusCode::NOT_FOUND => {
            RequestError::NotFound(format!("resource not found, url: {}", url))
        }
        StatusCode::TOO_MANY_REQUESTS => RequestError::TooManyRequests,
        s if s.is_client_error() => RequestError::BadRequest(format!(
            "http request to {url} failed, status: {status}, payload: {payload}",
        )),
        _ => RequestError::ServerError(format!(
            "http request to {url} failed, status: {status}, payload: {payload}",
        )),
    }
}

fn default_headers() -> Vec<(&'static str, &'static str)> {
    vec![
        ("user-agent", UA_VALUE),
        ("accept", "application/json;charset=UTF-8"),
        ("accept-encoding", "gzip, deflate, br"),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{body_json, body_string, header, method, path, query_param},
    };

    #[derive(Debug, Serialize, Deserialize, PartialEq)]
    struct TestResponse {
        id: u32,
        name: String,
    }

    #[derive(Debug, Serialize)]
    struct TestPayload {
        title: String,
    }

    #[test]
    fn test_default_headers() {
        let headers = default_headers();

        assert_eq!(headers.len(), 3);
        assert!(headers.contains(&("user-agent", UA_VALUE)));
        assert!(headers.contains(&("accept", "application/json;charset=UTF-8")));
        assert!(headers.contains(&("accept-encoding", "gzip, deflate, br")));
    }

    #[tokio::test]
    async fn test_get_success() {
        let mock_server = MockServer::start().await;
        let response_data = TestResponse {
            id: 1,
            name: "test".to_string(),
        };

        Mock::given(method("GET"))
            .and(path("/api/test"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&response_data))
            .mount(&mock_server)
            .await;

        let url = format!("{}/api/test", mock_server.uri());
        let result: RequestResult<TestResponse> = get(url, None, None).await;

        assert!(result.is_ok(), "Expected Ok, got: {:?}", result);
        let data = result.unwrap();
        assert_eq!(data.id, 1);
        assert_eq!(data.name, "test");
    }

    #[tokio::test]
    async fn test_get_with_query_params() {
        let mock_server = MockServer::start().await;
        let response_data = TestResponse {
            id: 2,
            name: "filtered".to_string(),
        };

        Mock::given(method("GET"))
            .and(path("/api/search"))
            .and(query_param("q", "rust"))
            .and(query_param("page", "1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&response_data))
            .mount(&mock_server)
            .await;

        let url = format!("{}/api/search", mock_server.uri());
        let query = Some(vec![("q", "rust"), ("page", "1")]);
        let result: RequestResult<TestResponse> = get(url, query, None).await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap().id, 2);
    }

    #[tokio::test]
    async fn test_get_with_custom_headers() {
        let mock_server = MockServer::start().await;
        let response_data = TestResponse {
            id: 3,
            name: "authenticated".to_string(),
        };

        Mock::given(method("GET"))
            .and(path("/api/protected"))
            .and(header("Authorization", "Bearer token123"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&response_data))
            .mount(&mock_server)
            .await;

        let url = format!("{}/api/protected", mock_server.uri());
        let headers = Some(vec![("Authorization", "Bearer token123")]);
        let result: RequestResult<TestResponse> = get(url, None, headers).await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap().name, "authenticated");
    }

    #[tokio::test]
    async fn test_get_not_found() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/api/missing"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&mock_server)
            .await;

        let url = format!("{}/api/missing", mock_server.uri());
        let result: RequestResult<TestResponse> = get(url, None, None).await;

        assert!(result.is_err());
        match result {
            Err(RequestError::NotFound(_)) => {}
            _ => panic!("Expected NotFound error"),
        }
    }

    #[tokio::test]
    async fn test_get_unauthorized() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/api/protected"))
            .respond_with(ResponseTemplate::new(401))
            .mount(&mock_server)
            .await;

        let url = format!("{}/api/protected", mock_server.uri());
        let result: RequestResult<TestResponse> = get(url, None, None).await;

        assert!(result.is_err());
        match result {
            Err(RequestError::Unauthorized) => {}
            _ => panic!("Expected Unauthorized error"),
        }
    }

    #[tokio::test]
    async fn test_get_invalid_json() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/api/bad"))
            .respond_with(ResponseTemplate::new(200).set_body_string("not json"))
            .mount(&mock_server)
            .await;

        let url = format!("{}/api/bad", mock_server.uri());
        let result: RequestResult<TestResponse> = get(url, None, None).await;

        assert!(result.is_err());
        match result {
            Err(RequestError::Other(msg)) => {
                assert!(msg.contains("decode payload failed"));
            }
            _ => panic!("Expected Other with decode message"),
        }
    }

    #[tokio::test]
    async fn test_post_success_with_payload() {
        let mock_server = MockServer::start().await;
        let response_data = TestResponse {
            id: 10,
            name: "created".to_string(),
        };
        let payload = TestPayload {
            title: "New Item".to_string(),
        };

        Mock::given(method("POST"))
            .and(path("/api/items"))
            .and(header("content-type", "application/json"))
            .and(body_json(&payload))
            .respond_with(ResponseTemplate::new(200).set_body_json(&response_data))
            .mount(&mock_server)
            .await;

        let url = format!("{}/api/items", mock_server.uri());
        let result: RequestResult<TestResponse> = post(url, None, None, Some(&payload)).await;

        assert!(result.is_ok());
        let data = result.unwrap();
        assert_eq!(data.id, 10);
        assert_eq!(data.name, "created");
    }

    #[tokio::test]
    async fn test_post_success_without_payload() {
        let mock_server = MockServer::start().await;
        let response_data = TestResponse {
            id: 11,
            name: "empty".to_string(),
        };

        Mock::given(method("POST"))
            .and(path("/api/trigger"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&response_data))
            .mount(&mock_server)
            .await;

        let url = format!("{}/api/trigger", mock_server.uri());
        let result: RequestResult<TestResponse> = post::<_, (), _>(url, None, None, None).await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap().id, 11);
    }

    #[tokio::test]
    async fn test_post_with_query_and_headers() {
        let mock_server = MockServer::start().await;
        let response_data = TestResponse {
            id: 12,
            name: "complex".to_string(),
        };
        let payload = TestPayload {
            title: "Test".to_string(),
        };

        Mock::given(method("POST"))
            .and(path("/api/items"))
            .and(query_param("version", "v1"))
            .and(header("X-Custom", "value"))
            .and(header("content-type", "application/json"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&response_data))
            .mount(&mock_server)
            .await;

        let url = format!("{}/api/items", mock_server.uri());
        let query = Some(vec![("version", "v1")]);
        let headers = Some(vec![("X-Custom", "value")]);
        let result: RequestResult<TestResponse> = post(url, query, headers, Some(&payload)).await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap().id, 12);
    }

    #[tokio::test]
    async fn test_post_server_error() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/error"))
            .respond_with(ResponseTemplate::new(500).set_body_string("Internal Server Error"))
            .mount(&mock_server)
            .await;

        let url = format!("{}/api/error", mock_server.uri());
        let result: RequestResult<TestResponse> = post::<_, (), _>(url, None, None, None).await;

        assert!(result.is_err());
        match result {
            Err(RequestError::ServerError(msg)) => {
                assert!(msg.contains("status: 500"));
            }
            _ => panic!("Expected ServerError with status 500"),
        }
    }

    #[tokio::test]
    async fn test_get_bad_request() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/api/bad"))
            .respond_with(ResponseTemplate::new(400).set_body_string("Bad Request"))
            .mount(&mock_server)
            .await;

        let url = format!("{}/api/bad", mock_server.uri());
        let result: RequestResult<TestResponse> = get(url, None, None).await;

        assert!(result.is_err());
        match result {
            Err(RequestError::BadRequest(msg)) => {
                assert!(msg.contains("status: 400"));
            }
            _ => panic!("Expected BadRequest, got: {:?}", result),
        }
    }

    #[tokio::test]
    async fn test_download_file_success() {
        let mock_server = MockServer::start().await;
        let file_content = b"test file content";

        Mock::given(method("GET"))
            .and(path("/files/test.txt"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(file_content))
            .mount(&mock_server)
            .await;

        let temp_dir = std::env::temp_dir();
        let file_path = temp_dir.join("test_download.txt");
        let file_path_str = file_path.to_str().unwrap();

        // Clean up if exists
        let _ = tokio::fs::remove_file(&file_path).await;

        let url = format!("{}/files/test.txt", mock_server.uri());
        let result = download_file(&url, file_path_str).await;

        assert!(result.is_ok());

        // Verify file was written correctly
        let content = tokio::fs::read(&file_path).await.unwrap();
        assert_eq!(content, file_content);

        // Clean up
        let _ = tokio::fs::remove_file(&file_path).await;
    }

    #[tokio::test]
    async fn test_download_file_creates_directory() {
        let mock_server = MockServer::start().await;
        let file_content = b"nested file";

        Mock::given(method("GET"))
            .and(path("/files/nested.txt"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(file_content))
            .mount(&mock_server)
            .await;

        let temp_dir = std::env::temp_dir();
        let nested_path = temp_dir
            .join("test_nested_dir")
            .join("subdir")
            .join("file.txt");
        let file_path_str = nested_path.to_str().unwrap();

        // Clean up if exists
        let _ = tokio::fs::remove_dir_all(temp_dir.join("test_nested_dir")).await;

        let url = format!("{}/files/nested.txt", mock_server.uri());
        let result = download_file(&url, file_path_str).await;

        assert!(result.is_ok());

        // Verify file was written correctly
        let content = tokio::fs::read(&nested_path).await.unwrap();
        assert_eq!(content, file_content);

        // Clean up
        let _ = tokio::fs::remove_dir_all(temp_dir.join("test_nested_dir")).await;
    }

    #[tokio::test]
    async fn test_download_file_not_found() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/files/missing.txt"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&mock_server)
            .await;

        let temp_dir = std::env::temp_dir();
        let file_path = temp_dir.join("missing.txt");
        let file_path_str = file_path.to_str().unwrap();

        let url = format!("{}/files/missing.txt", mock_server.uri());
        let result = download_file(&url, file_path_str).await;

        assert!(result.is_err());
        match result {
            Err(RequestError::NotFound(url)) => {
                assert!(url.contains("/files/missing.txt"));
            }
            _ => panic!("Expected NotFound error"),
        }
    }

    #[tokio::test]
    async fn test_download_file_server_error() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/files/error.txt"))
            .respond_with(ResponseTemplate::new(503).set_body_string("Service Unavailable"))
            .mount(&mock_server)
            .await;

        let temp_dir = std::env::temp_dir();
        let file_path = temp_dir.join("error.txt");
        let file_path_str = file_path.to_str().unwrap();

        let url = format!("{}/files/error.txt", mock_server.uri());
        let result = download_file(&url, file_path_str).await;

        assert!(result.is_err());
        match result {
            Err(RequestError::ServerError(msg)) => {
                assert!(msg.contains("status: 503"));
            }
            _ => panic!("Expected ServerError with status 503"),
        }
    }

    #[tokio::test]
    async fn test_download_file_bad_request() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/files/bad.txt"))
            .respond_with(ResponseTemplate::new(403).set_body_string("Forbidden"))
            .mount(&mock_server)
            .await;

        let temp_dir = std::env::temp_dir();
        let file_path = temp_dir.join("bad.txt");
        let file_path_str = file_path.to_str().unwrap();

        let url = format!("{}/files/bad.txt", mock_server.uri());
        let result = download_file(&url, file_path_str).await;

        assert!(result.is_err());
        match result {
            Err(RequestError::BadRequest(msg)) => {
                assert!(msg.contains("status: 403"));
            }
            _ => panic!("Expected BadRequest, got: {:?}", result),
        }
    }

    #[tokio::test]
    async fn test_get_text_success() {
        let mock_server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/page"))
            .respond_with(ResponseTemplate::new(200).set_body_string("<html>ok</html>"))
            .mount(&mock_server)
            .await;

        let url = format!("{}/page", mock_server.uri());
        let body = get_text(url, None).await.unwrap();
        assert_eq!(body, "<html>ok</html>");
    }

    #[tokio::test]
    async fn test_post_form_success() {
        let mock_server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/form"))
            .and(body_string("doctype=1&message=hi"))
            .respond_with(ResponseTemplate::new(200).set_body_string(r#"{"code":0}"#))
            .mount(&mock_server)
            .await;

        let url = format!("{}/form", mock_server.uri());
        let body = post_form(url, &[("doctype", "1"), ("message", "hi")], None)
            .await
            .unwrap();
        assert_eq!(body, r#"{"code":0}"#);
    }
}
