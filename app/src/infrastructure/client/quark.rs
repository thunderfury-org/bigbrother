use std::collections::HashMap;

use base64::Engine;
use serde::Deserialize;

use super::http;
use super::{RequestError, RequestResult};

const API_URL: &str = "https://pc-api.uc.cn";
const DOWNLOAD_API_URL: &str = "https://drive-pc.quark.cn";
const DESKTOP_UA: &str = concat!(
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) ",
    "AppleWebKit/537.36 (KHTML, like Gecko) quark-cloud-drive/2.5.56 ",
    "Chrome/100.0.4896.160 Electron/18.3.5.12-a038f7b798 Safari/537.36 ",
    "Channel/pckk_other_ch",
);
const BROWSER_UA: &str = concat!(
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) ",
    "AppleWebKit/537.36 (KHTML, like Gecko) Chrome/143.0.0.0 Safari/537.36",
);

const QUARK_CODE_SHARE_CANCELLED: i32 = 41012;
const QUARK_CODE_FILE_SIZE_LIMIT: i32 = 23018;

#[derive(Debug, Clone)]
pub struct Client {
    cookie: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Folder {
    pub fid: String,
    pub file_name: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct File {
    pub fid: String,
    pub file_name: String,
    pub size: u64,
    pub share_fid_token: String,
    #[serde(default)]
    pub dir: bool,
}

#[derive(Debug, Deserialize)]
struct QuarkResponse<T> {
    code: i32,
    #[serde(default)]
    message: String,
    data: Option<T>,
}

#[derive(Debug, Deserialize)]
struct ShareTokenData {
    stoken: String,
}

#[derive(Debug, Deserialize)]
struct FileListData {
    list: Vec<File>,
}

#[derive(Debug, Deserialize)]
struct DownloadItem {
    fid: String,
    md5: String,
}

impl Client {
    pub fn new(cookie: &str) -> Self {
        Self {
            cookie: cookie.to_owned(),
        }
    }

    fn headers(&self) -> Vec<(&str, &str)> {
        vec![
            ("cookie", self.cookie.as_str()),
            ("accept", "application/json, text/plain, */*"),
        ]
    }

    pub async fn get_share_info(&self, share_id: &str, passcode: &str) -> RequestResult<String> {
        let url = format!("{API_URL}/1/clouddrive/share/sharepage/token");
        let resp: QuarkResponse<ShareTokenData> = http::post(
            &url,
            None,
            Some(self.headers()),
            Some(&serde_json::json!({
                "pwd_id": share_id,
                "passcode": passcode,
            })),
        )
        .await?;
        parse_quark_response(resp, "get_share_info")?
            .ok_or_else(|| RequestError::Other("quark get_share_info returned no data".into()))
            .map(|d| d.stoken)
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
        let resp: QuarkResponse<FileListData> = http::get(
            &url,
            Some(vec![
                ("pwd_id", share_id),
                ("passcode", passcode),
                ("stoken", stoken),
                ("pdir_fid", pdir_fid),
                ("force", "0"),
                ("_page", &page_str),
                ("_size", &size_str),
                ("_fetch_banner", "0"),
                ("_fetch_share", "0"),
                ("_fetch_total", "1"),
                ("_sort", "file_type:asc,updated_at:desc"),
            ]),
            Some(self.headers()),
        )
        .await?;
        let data = parse_quark_response(resp, "list_share_files")?
            .ok_or_else(|| RequestError::Other("quark list_share_files returned no data".into()))?;

        let mut folders = Vec::new();
        let mut files = Vec::new();
        for item in data.list {
            if item.dir {
                folders.push(Folder {
                    fid: item.fid,
                    file_name: item.file_name,
                });
            } else {
                files.push(File {
                    fid: item.fid,
                    file_name: item.file_name,
                    size: item.size,
                    share_fid_token: item.share_fid_token,
                    dir: false,
                });
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
        let url = format!("{DOWNLOAD_API_URL}/1/clouddrive/file/download");
        let payload = serde_json::json!({
            "fids": fids,
            "pwd_id": share_id,
            "stoken": stoken,
            "fids_token": fid_tokens,
            "passcode": passcode,
        });
        let query = vec![
            ("pr", "ucpro"),
            ("fr", "pc"),
            ("sys", "win32"),
            ("ve", "2.5.56"),
            ("ut", ""),
            ("guid", ""),
        ];

        // Download API needs specific headers and bypasses normal HTTP status checking
        // (quark returns 400 with JSON error code 23018 for large files).
        // Using http::HTTP_CLIENT directly to match original working behavior.
        let resp = self
            .raw_download_request(&url, &query, &payload, BROWSER_UA)
            .await?;
        let resp = if resp.code == QUARK_CODE_FILE_SIZE_LIMIT {
            self.raw_download_request(&url, &query, &payload, DESKTOP_UA)
                .await?
        } else {
            resp
        };

        let items = parse_quark_response(resp, "batch_download_info")?;
        let mut result = HashMap::new();
        if let Some(items) = items {
            for item in items {
                result.insert(item.fid, decode_md5(&item.md5));
            }
        }
        Ok(result)
    }

    async fn raw_download_request(
        &self,
        url: &str,
        query: &[(&str, &str)],
        payload: &serde_json::Value,
        user_agent: &str,
    ) -> RequestResult<QuarkResponse<Vec<DownloadItem>>> {
        let response = http::post_raw(
            url,
            Some(query.to_vec()),
            vec![
                ("cookie", self.cookie.as_str()),
                ("accept", "application/json, text/plain, */*"),
                ("user-agent", user_agent),
                ("referer", "https://pan.quark.cn/"),
                ("accept-language", "zh-CN"),
            ],
            payload,
        )
        .await?;
        let text = response.text().await.unwrap_or_default();
        serde_json::from_str::<QuarkResponse<Vec<DownloadItem>>>(&text).map_err(|e| {
            RequestError::Other(format!(
                "quark download decode failed: {e}, body: {}",
                &text[..text.len().min(200)]
            ))
        })
    }
}

fn parse_quark_response<T>(resp: QuarkResponse<T>, api: &str) -> RequestResult<Option<T>> {
    match resp.code {
        0 => Ok(resp.data),
        QUARK_CODE_SHARE_CANCELLED => Err(RequestError::ShareCancelled(resp.message)),
        _ => Err(RequestError::Other(format!(
            "quark {api} failed, code: {}, message: {}",
            resp.code, resp.message
        ))),
    }
}

fn decode_md5(md5: &str) -> String {
    if let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(md5)
        && bytes.len() == 16
    {
        return bytes.iter().map(|b| format!("{b:02x}")).collect();
    }
    md5.to_owned()
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
