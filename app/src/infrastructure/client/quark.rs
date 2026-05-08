#![allow(dead_code)]

use std::{collections::HashMap, sync::LazyLock, time::Duration};

use base64::Engine;
use reqwest_middleware::{ClientBuilder, ClientWithMiddleware};
use reqwest_retry::{RetryTransientMiddleware, policies::ExponentialBackoff};
use serde::Deserialize;

use super::{RequestError, RequestResult};

const API_URL: &str = "https://pc-api.uc.cn";

static HTTP_CLIENT: LazyLock<ClientWithMiddleware> = LazyLock::new(|| {
    let req_client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .expect("failed to create quark http client");
    let retry_policy = ExponentialBackoff::builder().build_with_max_retries(3);
    ClientBuilder::new(req_client)
        .with(RetryTransientMiddleware::new_with_policy(retry_policy))
        .build()
});

#[derive(Debug, Clone)]
pub struct Client {
    cookie: String,
}

#[derive(Debug, Deserialize)]
pub struct Folder {
    pub fid: String,
    pub file_name: String,
}

#[derive(Debug, Deserialize)]
pub struct File {
    pub fid: String,
    pub file_name: String,
    pub size: u64,
    pub share_fid_token: String,
    #[serde(default)]
    pub dir: bool,
}

#[derive(Debug, Deserialize)]
struct ShareTokenResponse {
    code: i32,
    #[serde(default)]
    message: String,
    data: Option<ShareTokenData>,
}

#[derive(Debug, Deserialize)]
struct ShareTokenData {
    stoken: String,
}

#[derive(Debug, Deserialize)]
struct FileListResponse {
    code: i32,
    #[serde(default)]
    message: String,
    data: Option<FileListData>,
}

#[derive(Debug, Deserialize)]
struct FileListData {
    list: Vec<FileItem>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum FileItem {
    Folder(Folder),
    File(File),
}

#[derive(Debug, Deserialize)]
struct DownloadResponse {
    code: i32,
    #[serde(default)]
    message: String,
    data: Option<Vec<DownloadItem>>,
}

#[derive(Debug, Deserialize)]
struct DownloadItem {
    fid: String,
    md5: String,
}

const QUARK_CODE_SHARE_CANCELLED: i32 = 41012;

fn quark_error(code: i32, message: &str, api: &str) -> RequestError {
    match code {
        QUARK_CODE_SHARE_CANCELLED => RequestError::ShareCancelled(message.to_owned()),
        _ => RequestError::Error(format!(
            "quark {api} failed, code: {code}, message: {message}"
        )),
    }
}

impl Client {
    pub fn new(cookie: &str) -> Self {
        Self {
            cookie: cookie.to_owned(),
        }
    }

    fn default_headers(&self) -> Vec<(&str, &str)> {
        vec![
            ("cookie", self.cookie.as_str()),
            ("content-type", "application/json"),
            ("accept", "application/json, text/plain, */*"),
            (
                "user-agent",
                "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36",
            ),
        ]
    }

    async fn send_request<T: serde::de::DeserializeOwned>(
        &self,
        method: reqwest::Method,
        url: &str,
        query: Option<&[(&str, &str)]>,
        payload: Option<&serde_json::Value>,
    ) -> RequestResult<T> {
        let mut request = match method {
            reqwest::Method::GET => HTTP_CLIENT.get(url),
            reqwest::Method::POST => HTTP_CLIENT.post(url),
            _ => return Err(RequestError::Error(format!("unsupported method: {method}"))),
        };
        for (k, v) in self.default_headers() {
            request = request.header(k, v);
        }
        if let Some(q) = query {
            request = request.query(q);
        }
        if let Some(p) = payload {
            let body = serde_json::to_vec(p)
                .map_err(|e| RequestError::Error(format!("serialize quark payload failed, {e}")))?;
            request = request.body(body);
        }

        let response = request.send().await?;
        let status = response.status();
        let text = response.text().await.unwrap_or_default();

        if !status.is_success() {
            return Err(RequestError::Error(format!(
                "quark request to {url} failed, status: {status}, body: {}",
                &text[..text.len().min(200)]
            )));
        }

        serde_json::from_str(&text).map_err(|e| {
            RequestError::Error(format!(
                "quark request to {url} failed to decode response: {e}, body: {}",
                &text[..text.len().min(200)]
            ))
        })
    }

    pub async fn get_share_info(&self, share_id: &str, passcode: &str) -> RequestResult<String> {
        let url = format!("{API_URL}/1/clouddrive/share/sharepage/token");
        let payload = serde_json::json!({
            "pwd_id": share_id,
            "passcode": passcode,
        });
        let resp: ShareTokenResponse = self
            .send_request(reqwest::Method::POST, &url, None, Some(&payload))
            .await?;

        if resp.code != 0 {
            return Err(quark_error(resp.code, &resp.message, "get_share_info"));
        }

        resp.data
            .map(|d| d.stoken)
            .ok_or_else(|| RequestError::Error("quark get_share_info returned no data".into()))
    }

    pub async fn list_share_files(
        &self,
        share_id: &str,
        passcode: &str,
        stoken: &str,
        pdir_fid: &str,
        page: i32,
        size: i32,
    ) -> RequestResult<(Vec<Folder>, Vec<File>)> {
        let url = format!("{API_URL}/1/clouddrive/share/sharepage/detail");
        let page_str = page.to_string();
        let size_str = size.to_string();
        let query = [
            ("pwd_id", share_id),
            ("passcode", passcode),
            ("stoken", stoken),
            ("pdir_fid", pdir_fid),
            ("force", "0"),
            ("_page", page_str.as_str()),
            ("_size", size_str.as_str()),
            ("_fetch_banner", "0"),
            ("_fetch_share", "0"),
            ("_fetch_total", "1"),
            ("_sort", "file_type:asc,updated_at:desc"),
        ];
        let resp: FileListResponse = self
            .send_request(reqwest::Method::GET, &url, Some(&query), None)
            .await?;

        if resp.code != 0 {
            return Err(quark_error(resp.code, &resp.message, "list_share_files"));
        }

        let data = resp
            .data
            .ok_or_else(|| RequestError::Error("quark list_share_files returned no data".into()))?;

        let mut folders = Vec::new();
        let mut files = Vec::new();
        for item in data.list {
            match item {
                FileItem::Folder(f) => folders.push(f),
                FileItem::File(f) if !f.dir => files.push(f),
                _ => {}
            }
        }

        Ok((folders, files))
    }

    pub async fn batch_download_info(
        &self,
        share_id: &str,
        passcode: &str,
        stoken: &str,
        fids: &[String],
        fid_tokens: &[String],
    ) -> RequestResult<HashMap<String, String>> {
        let url = format!("{API_URL}/1/clouddrive/file/download");
        let query = [("entry", "ft"), ("uc_param_str", "")];
        let payload = serde_json::json!({
            "fids": fids,
            "pwd_id": share_id,
            "stoken": stoken,
            "fids_token": fid_tokens,
            "passcode": passcode,
        });
        let resp: DownloadResponse = self
            .send_request(reqwest::Method::POST, &url, Some(&query), Some(&payload))
            .await?;

        if resp.code != 0 {
            return Err(quark_error(resp.code, &resp.message, "batch_download_info"));
        }

        let mut result = HashMap::new();
        if let Some(items) = resp.data {
            for item in items {
                let md5_hex = decode_md5(&item.md5);
                result.insert(item.fid, md5_hex);
            }
        }

        Ok(result)
    }
}

fn decode_md5(md5: &str) -> String {
    if md5.contains("==") {
        base64::engine::general_purpose::STANDARD
            .decode(md5)
            .map(|bytes| bytes.iter().map(|b| format!("{b:02x}")).collect())
            .unwrap_or_else(|_| md5.to_owned())
    } else {
        md5.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_base64_md5_to_hex() {
        let raw = [
            0xff, 0xd9, 0xa9, 0xbc, 0x76, 0x16, 0x54, 0x0f, 0xcd, 0x74, 0x1a, 0xfe, 0xe2, 0x23,
            0xa1, 0x2b,
        ];
        let b64 = base64::engine::general_purpose::STANDARD.encode(raw);
        let hex = decode_md5(&b64);
        assert_eq!(hex, "ffd9a9bc7616540fcd741afee223a12b");
    }

    #[test]
    fn passthrough_plain_hex_md5() {
        assert_eq!(
            decode_md5("ffd9a9bc7616540fcd741afee223a12b"),
            "ffd9a9bc7616540fcd741afee223a12b"
        );
    }
}
