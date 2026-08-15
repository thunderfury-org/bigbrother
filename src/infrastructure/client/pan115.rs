use std::{sync::Arc, time::Duration};

use serde::{Deserialize, Deserializer};
use serde_json::Value;
use tokio::sync::Mutex;

use super::{RequestError, RequestResult, http};

const API_URL: &str = "https://115cdn.com/webapi/share/snap";
const DEFAULT_REQUEST_INTERVAL: Duration = Duration::from_millis(1500);

// Custom deserializer to handle both string and number types
fn string_or_number<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: Deserializer<'de>,
{
    use serde::de::Error;
    use serde_json::Value;

    let value = Value::deserialize(deserializer)?;
    match value {
        Value::String(s) => Ok(Some(s)),
        Value::Number(n) => Ok(Some(n.to_string())),
        Value::Null => Ok(None),
        _ => Err(Error::custom("expected string or number")),
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct FileEntry {
    /// File ID (present for files)
    #[serde(default, rename = "fid", deserialize_with = "string_or_number")]
    pub fid: Option<String>,

    /// Folder ID (present for folders)
    #[serde(default, rename = "cid", deserialize_with = "string_or_number")]
    pub cid: Option<String>,

    /// Name
    #[serde(rename = "n")]
    pub name: String,

    /// Size
    #[serde(rename = "s")]
    pub size: u64,

    /// SHA1 hash (for files)
    #[serde(default, rename = "sha")]
    pub sha: Option<String>,
}

impl FileEntry {
    pub fn is_file(&self) -> bool {
        self.fid.is_some()
    }
}

#[derive(Debug, Deserialize)]
struct ListResponse {
    #[serde(rename = "state")]
    state: bool,
    #[serde(rename = "error")]
    error: String,
    #[serde(rename = "errno")]
    errno: i32,

    #[serde(rename = "data")]
    data: Option<Value>,
}

#[derive(Debug, Deserialize)]
struct ListResponseData {
    #[serde(rename = "count")]
    pub count: i32,

    #[serde(rename = "list")]
    pub list: Vec<FileEntry>,
}

#[derive(Debug, Clone)]
pub struct Client {
    api_url: Arc<str>,
    limiter: Arc<RequestLimiter>,
}

impl Default for Client {
    fn default() -> Self {
        Self::new()
    }
}

impl Client {
    pub fn new() -> Self {
        Self::with_request_interval(DEFAULT_REQUEST_INTERVAL)
    }

    pub fn with_request_interval(min_interval: Duration) -> Self {
        Self {
            api_url: Arc::from(API_URL),
            limiter: Arc::new(RequestLimiter::new(min_interval)),
        }
    }

    /// List files and folders in a shared directory
    ///
    /// # Arguments
    ///
    /// * `share_code` - The share code
    /// * `receive_code` - The receive code
    /// * `cid` - The container/folder ID (0 for root)
    pub async fn list_share_files(
        &self,
        share_code: &str,
        receive_code: &str,
        cid: &str,
    ) -> RequestResult<Vec<FileEntry>> {
        let mut files = Vec::new();

        let limit = 1000;
        let mut offset = 0;

        loop {
            self.limiter.acquire().await;
            let response = http::get_response(
                self.api_url.as_ref(),
                Some(vec![
                    ("share_code", share_code),
                    ("offset", offset.to_string().as_str()),
                    ("limit", limit.to_string().as_str()),
                    ("asc", "0"),
                    ("cid", cid),
                    ("receive_code", receive_code),
                    ("format", "json"),
                ]),
                None,
            )
            .await?;
            let response: ListResponse = decode_list_response(response).await?;

            if !response.state {
                return Err(map_list_error(response.errno, &response.error));
            }

            let data = response
                .data
                .ok_or_else(|| RequestError::Other("no data in response".to_string()))
                .and_then(decode_list_response_data)?;

            files.extend(data.list);

            // Check if there are more items
            if data.count < limit {
                break;
            }

            offset += limit;
        }

        Ok(files)
    }

    #[cfg(test)]
    fn new_for_test(api_url: impl Into<String>) -> Self {
        Self::new_for_test_with_interval(api_url, DEFAULT_REQUEST_INTERVAL)
    }

    #[cfg(test)]
    fn new_for_test_with_interval(api_url: impl Into<String>, min_interval: Duration) -> Self {
        Self {
            api_url: Arc::from(api_url.into()),
            limiter: Arc::new(RequestLimiter::new(min_interval)),
        }
    }

    #[cfg(test)]
    pub(crate) async fn min_request_interval(&self) -> Duration {
        self.limiter.min_interval()
    }
}

#[derive(Debug)]
struct RequestLimiter {
    min_interval: Duration,
    state: Mutex<Option<tokio::time::Instant>>,
}

impl RequestLimiter {
    fn new(min_interval: Duration) -> Self {
        Self {
            min_interval,
            state: Mutex::new(None),
        }
    }

    async fn acquire(&self) {
        let mut last_request_at = self.state.lock().await;
        if let Some(previous) = *last_request_at {
            let next_allowed_at = previous + self.min_interval;
            let now = tokio::time::Instant::now();
            if now < next_allowed_at {
                tokio::time::sleep_until(next_allowed_at).await;
            }
        }
        *last_request_at = Some(tokio::time::Instant::now());
    }

    #[cfg(test)]
    fn min_interval(&self) -> Duration {
        self.min_interval
    }
}

async fn decode_list_response(response: reqwest::Response) -> RequestResult<ListResponse> {
    let status = response.status();
    let url = response.url().to_string();
    let payload = response.text().await?;

    if status.is_success() {
        return serde_json::from_str::<ListResponse>(&payload).map_err(|err| {
            RequestError::Other(format!(
                "http request to {url} failed, decode payload failed, {err}, payload: {payload}",
            ))
        });
    }

    if status == reqwest::StatusCode::METHOD_NOT_ALLOWED && looks_like_pan115_risk_control(&payload)
    {
        return Err(RequestError::TooManyRequests);
    }

    match status {
        reqwest::StatusCode::UNAUTHORIZED => Err(RequestError::Unauthorized),
        reqwest::StatusCode::NOT_FOUND => Err(RequestError::NotFound(format!(
            "resource not found, url: {url}"
        ))),
        reqwest::StatusCode::TOO_MANY_REQUESTS => Err(RequestError::TooManyRequests),
        s if s.is_client_error() => Err(RequestError::BadRequest(format!(
            "http request to {url} failed, status: {status}, payload: {payload}",
        ))),
        _ => Err(RequestError::ServerError(format!(
            "http request to {url} failed, status: {status}, payload: {payload}",
        ))),
    }
}

fn looks_like_pan115_risk_control(payload: &str) -> bool {
    payload.contains("访问被阻断")
        || payload.contains("block_message")
        || payload.contains("potential threats to the server's security")
}

fn decode_list_response_data(data: Value) -> RequestResult<ListResponseData> {
    serde_json::from_value(data)
        .map_err(|err| RequestError::Other(format!("decode share list data failed, {err}")))
}

fn map_list_error(errno: i32, error: &str) -> RequestError {
    match errno {
        4100010 => RequestError::ShareCancelled(format!(
            "pan115 share cancelled, errno: {errno}, error: {error}"
        )),
        4100012 => RequestError::BadRequest(format!(
            "pan115 share requires receive code, errno: {errno}, error: {error}"
        )),
        _ => RequestError::Other(format!(
            "list share files failed, errno: {errno}, error: {error}"
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    };
    use std::time::{Duration, SystemTime, UNIX_EPOCH};
    use wiremock::{
        Mock, MockServer, Request, Respond, ResponseTemplate,
        matchers::{method, path, query_param},
    };

    struct TimestampResponder {
        first_seen_at_ms: Arc<AtomicU64>,
        second_seen_at_ms: Arc<AtomicU64>,
    }

    impl Respond for TimestampResponder {
        fn respond(&self, _request: &Request) -> ResponseTemplate {
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64;
            if self
                .first_seen_at_ms
                .compare_exchange(0, now, Ordering::SeqCst, Ordering::SeqCst)
                .is_err()
            {
                self.second_seen_at_ms.store(now, Ordering::SeqCst);
            }

            ResponseTemplate::new(200).set_body_string(
                r#"{
                    "state": true,
                    "error": "",
                    "errno": 0,
                    "data": {
                        "count": 0,
                        "list": []
                    }
                }"#,
            )
        }
    }

    #[test]
    fn test_file_entry_is_file() {
        let entry = FileEntry {
            fid: Some("123".to_string()),
            cid: None,
            name: "test.mp4".to_string(),
            size: 1024,
            sha: Some("abc123".to_string()),
        };

        assert!(entry.is_file());
    }

    #[test]
    fn test_file_entry_is_dir() {
        let entry = FileEntry {
            fid: None,
            cid: Some("456".to_string()),
            name: "folder".to_string(),
            size: 0,
            sha: None,
        };

        assert!(!entry.is_file());
    }

    #[test]
    fn test_file_entry_neither_file_nor_dir() {
        let entry = FileEntry {
            fid: None,
            cid: None,
            name: "unknown".to_string(),
            size: 100,
            sha: None,
        };

        assert!(!entry.is_file());
    }

    #[test]
    fn test_deserialize_file_entry_as_file() {
        let json = r#"{
            "fid": "3351075412498185300",
            "cid": "3351075729570791276",
            "n": "Anaconda.2025.2160p.iT.WEB-DL.DDP5.1.Atmos.DV.HDR.H.265.mkv",
            "s": 18617794907,
            "sha": "09619DF88530E74163823E54A78AD3412E2B82FC",
            "ico": "mkv"
        }"#;

        let entry: FileEntry = serde_json::from_str(json).unwrap();
        assert_eq!(entry.fid, Some("3351075412498185300".to_string()));
        assert_eq!(
            entry.name,
            "Anaconda.2025.2160p.iT.WEB-DL.DDP5.1.Atmos.DV.HDR.H.265.mkv"
        );
        assert_eq!(entry.size, 18617794907);
        assert_eq!(
            entry.sha,
            Some("09619DF88530E74163823E54A78AD3412E2B82FC".to_string())
        );
        assert!(entry.is_file());
    }

    #[test]
    fn test_deserialize_file_entry_as_folder() {
        let json = r#"{
            "cid": "3351075729570791276",
            "pid": "0",
            "n": "新狂蟒之灾 Anaconda (2025)",
            "s": 18617794907,
            "fc": 1,
            "t": "1769495365"
        }"#;

        let entry: FileEntry = serde_json::from_str(json).unwrap();
        assert_eq!(entry.cid, Some("3351075729570791276".to_string()));
        assert_eq!(entry.name, "新狂蟒之灾 Anaconda (2025)");
        assert!(!entry.is_file());
    }

    #[test]
    fn test_deserialize_list_response() {
        let json = r#"{
            "state": true,
            "error": "",
            "errno": 0,
            "data": {
                "userinfo": {
                    "user_id": "344385180",
                    "user_name": "3***0",
                    "face": "http://avatars.115.com/01/3c86md_m.jpg"
                },
                "shareinfo": {
                    "snap_id": "312951397",
                    "file_size": 18617794907,
                    "share_title": "新狂蟒之灾 Anaconda (2025)",
                    "share_state": 1,
                    "receive_count": 178
                },
                "count": 1,
                "list": [
                    {
                        "cid": "3351075729570791276",
                        "pid": "0",
                        "n": "新狂蟒之灾 Anaconda (2025)",
                        "s": 18617794907,
                        "fc": 1
                    }
                ]
            }
        }"#;

        let response: ListResponse = serde_json::from_str(json).unwrap();
        assert!(response.state);
        assert_eq!(response.errno, 0);
        let data = decode_list_response_data(response.data.unwrap()).unwrap();
        assert_eq!(data.count, 1);
        assert_eq!(data.list.len(), 1);
        assert_eq!(data.list[0].name, "新狂蟒之灾 Anaconda (2025)");
    }

    #[test]
    fn test_deserialize_list_response_requires_receive_code() {
        let json = r#"{
            "state": false,
            "error": "请输入访问码",
            "errno": 4100012,
            "data": {
                "userinfo": {
                    "user_id": "309192325",
                    "user_name": "星***视"
                },
                "is_access": 0
            },
            "errtype": ""
        }"#;

        let response: ListResponse = serde_json::from_str(json).unwrap();
        assert!(!response.state);

        match map_list_error(response.errno, &response.error) {
            RequestError::BadRequest(message) => {
                assert!(message.contains("requires receive code"));
                assert!(message.contains("4100012"));
            }
            other => panic!("Expected BadRequest, got: {other:?}"),
        }
    }

    #[test]
    fn map_list_error_classifies_share_cancelled_errno() {
        match map_list_error(4100010, "分享已取消") {
            RequestError::ShareCancelled(message) => {
                assert!(message.contains("分享已取消"));
            }
            other => panic!("Expected ShareCancelled, got: {other:?}"),
        }
    }

    #[test]
    fn test_client_new() {
        let _client = Client::new();
        let _client_default = Client::default();
    }

    #[test]
    fn test_deserialize_cid_as_number() {
        let json = r#"{
            "cid": 3351075729570791276,
            "n": "Test Folder",
            "s": 12345
        }"#;

        let entry: FileEntry = serde_json::from_str(json).unwrap();
        assert_eq!(entry.cid, Some("3351075729570791276".to_string()));
        assert_eq!(entry.name, "Test Folder");
    }

    #[test]
    fn test_deserialize_fid_as_number() {
        let json = r#"{
            "fid": 9876543210123456789,
            "n": "Test File",
            "s": 54321
        }"#;

        let entry: FileEntry = serde_json::from_str(json).unwrap();
        assert_eq!(entry.fid, Some("9876543210123456789".to_string()));
        assert_eq!(entry.name, "Test File");
    }

    #[test]
    fn test_deserialize_mixed_types_from_real_api() {
        // Real API response with cid as number in file entry
        let json = r#"{
            "fid": "3351075412498185300",
            "cid": 3351075729570791276,
            "n": "Anaconda.2025.mkv",
            "s": 18617794907,
            "sha": "09619DF88530E74163823E54A78AD3412E2B82FC"
        }"#;

        let entry: FileEntry = serde_json::from_str(json).unwrap();
        assert_eq!(entry.fid, Some("3351075412498185300".to_string()));
        assert_eq!(entry.cid, Some("3351075729570791276".to_string()));
        assert_eq!(entry.name, "Anaconda.2025.mkv");
        assert!(entry.is_file());
    }

    #[tokio::test]
    async fn list_share_files_treats_pan115_risk_control_as_too_many_requests() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/webapi/share/snap"))
            .and(query_param("share_code", "share115"))
            .and(query_param("receive_code", "recv"))
            .and(query_param("cid", "0"))
            .respond_with(ResponseTemplate::new(405).set_body_string(
                r#"<!doctypehtml><html lang="zh-cn"><body>
                    很抱歉，由于您访问的URL有可能对网站造成安全威胁，您的访问被阻断。
                    </body></html>"#,
            ))
            .mount(&server)
            .await;

        let client = Client::new_for_test(format!("{}/webapi/share/snap", server.uri()));
        let result = client.list_share_files("share115", "recv", "0").await;

        let err = result.unwrap_err();
        assert!(
            matches!(err, RequestError::TooManyRequests),
            "expected TooManyRequests, got {err:?}"
        );
    }

    #[tokio::test]
    async fn list_share_files_rate_limits_across_cloned_clients() {
        let server = MockServer::start().await;
        let first_seen_at_ms = Arc::new(AtomicU64::new(0));
        let second_seen_at_ms = Arc::new(AtomicU64::new(0));

        Mock::given(method("GET"))
            .and(path("/webapi/share/snap"))
            .respond_with(TimestampResponder {
                first_seen_at_ms: first_seen_at_ms.clone(),
                second_seen_at_ms: second_seen_at_ms.clone(),
            })
            .expect(2)
            .mount(&server)
            .await;

        let client = Client::new_for_test_with_interval(
            format!("{}/webapi/share/snap", server.uri()),
            Duration::from_millis(200),
        );
        let cloned = client.clone();

        client
            .list_share_files("share115", "recv", "0")
            .await
            .unwrap();
        cloned
            .list_share_files("share115", "recv", "0")
            .await
            .unwrap();

        let first = first_seen_at_ms.load(Ordering::SeqCst);
        let second = second_seen_at_ms.load(Ordering::SeqCst);
        assert!(
            first > 0 && second > 0,
            "expected both requests to be observed"
        );
        let observed_gap = second.saturating_sub(first);
        assert!(
            observed_gap >= 180,
            "expected second request to be delayed, first={first}, second={second}, gap={observed_gap}"
        );
    }
}
