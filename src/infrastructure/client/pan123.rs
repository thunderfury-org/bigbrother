use std::{collections::HashMap, fs, path::Path, sync::Arc};

use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::json;
use tokio::sync::RwLock;

use super::{RequestError, RequestResult, http};

const OPEN_API_BASE: &str = "https://open-api.123pan.com";
const WEB_API_BASE: &str = "https://yun.123pan.com/b";

const PLATFORM_KEY: &str = "Platform";
const PLATFORM_VALUE: &str = "open_platform";

const TOKEN_CACHE_FILE: &str = "open_api_token.json";

#[derive(Debug, Deserialize)]
struct CommonResponse<T> {
    code: i32,
    message: String,
    data: Option<T>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct File {
    #[serde(rename = "FileId", alias = "fileId", alias = "fileID")]
    pub file_id: i64,
    #[serde(rename = "FileName", alias = "filename")]
    pub file_name: String,
    /// 0: file, 1: folder
    #[serde(rename = "Type", alias = "type")]
    pub file_type: i32,
    #[serde(rename = "Size", alias = "size")]
    pub size: u64,
    #[serde(rename = "Etag", alias = "etag")]
    pub etag: String,
    #[allow(dead_code)]
    #[serde(default, alias = "parentFileId")]
    pub parent_file_id: Option<i64>,
    /// 0: normal, 1: in trash
    #[serde(default)]
    pub trashed: i32,
}

impl File {
    pub fn is_dir(&self) -> bool {
        self.file_type == 1
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchPathFile {
    pub file_id: i64,
    pub file_name: String,
    pub is_dir: bool,
    pub path: String,
}

#[derive(Debug, Deserialize)]
struct OpenApiFileListResponse {
    #[serde(default, rename = "lastFileId")]
    last_file_id: i64,
    #[serde(default, rename = "InfoList", alias = "fileList")]
    file_list: Vec<File>,
}

#[derive(Debug, Deserialize)]
struct FastUploadResponse {
    #[serde(alias = "Reuse", alias = "reuse")]
    reuse: bool,
    #[serde(alias = "fileID", alias = "FileId")]
    file_id: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct MkdirResponse {
    #[serde(rename = "dirID")]
    dir_id: i64,
}

#[derive(Debug, Deserialize)]
struct TrashResponse {}

#[derive(Debug, Deserialize)]
struct FileDetail {
    #[serde(rename = "FileId", alias = "fileId", alias = "fileID")]
    file_id: i64,
    #[serde(rename = "FileName", alias = "filename")]
    file_name: String,
    #[serde(default, alias = "parentFileID", alias = "parentFileId")]
    parent_file_id: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct MultiGetResponse {
    #[serde(rename = "infoList", alias = "fileList")]
    file_list: Vec<FileDetail>,
}

#[derive(Debug, Deserialize)]
struct DownloadInfo {
    #[serde(rename = "downloadUrl")]
    download_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CachedToken {
    #[serde(rename = "token")]
    access_token: String,
    #[serde(default)]
    refresh_token: String,
    #[serde(rename = "expire", with = "time::serde::rfc3339")]
    expired_at: time::OffsetDateTime,
}

#[derive(Debug, Deserialize)]
struct RefreshTokenResp {
    #[serde(rename = "access_token")]
    access_token: String,
    #[serde(rename = "refresh_token")]
    refresh_token: String,
    #[serde(rename = "expires_in")]
    expires_in: i64,
    #[serde(default)]
    code: i32,
    #[serde(default)]
    message: String,
    #[serde(default)]
    error_description: String,
    #[serde(default)]
    error: String,
    #[serde(default)]
    text: String,
}

#[derive(Debug, Default, Clone)]
pub struct Client {
    api_address: String,
    initial_refresh_token: String,
    cache_dir: String,
    open_api_base: String,
    token: Arc<RwLock<Option<CachedToken>>>,
}

impl Client {
    pub fn new(api_address: &str, refresh_token: &str, cache_dir: &str) -> Self {
        Self {
            api_address: api_address.to_owned(),
            initial_refresh_token: refresh_token.to_owned(),
            cache_dir: cache_dir.to_owned(),
            open_api_base: OPEN_API_BASE.to_owned(),
            token: Arc::new(RwLock::new(None)),
        }
    }

    #[cfg(test)]
    pub(crate) fn with_open_api_base(
        api_address: &str,
        refresh_token: &str,
        cache_dir: &str,
        open_api_base: &str,
    ) -> Self {
        Self {
            api_address: api_address.to_owned(),
            initial_refresh_token: refresh_token.to_owned(),
            cache_dir: cache_dir.to_owned(),
            open_api_base: open_api_base.to_owned(),
            token: Arc::new(RwLock::new(None)),
        }
    }

    #[cfg(test)]
    pub(crate) async fn set_token_for_test(&self, token: &str, expired_at: time::OffsetDateTime) {
        let mut guard = self.token.write().await;
        *guard = Some(CachedToken {
            access_token: token.to_owned(),
            refresh_token: String::new(),
            expired_at,
        });
    }

    pub async fn get_download_url(&self, file_id: i64) -> RequestResult<String> {
        let download_info = self
            .get::<DownloadInfo>(
                self.build_api_url("/api/v1/file/download_info"),
                Some(vec![("fileId", file_id.to_string().as_str())]),
            )
            .await?;
        if download_info.download_url.is_empty() {
            Err(RequestError::Other(
                "get download url failed, empty download url".to_string(),
            ))
        } else {
            Ok(download_info.download_url)
        }
    }

    pub async fn download_file(&self, file_id: i64, local_file_path: &str) -> RequestResult<()> {
        let download_url = self.get_download_url(file_id).await?;
        http::download_file(download_url.as_str(), local_file_path).await
    }

    pub async fn list(&self, file_id: i64) -> RequestResult<Vec<File>> {
        let parent_file_id = file_id.to_string();
        let mut last_file_id = 0i64;
        let mut files = Vec::new();

        loop {
            let last_file_id_value = last_file_id.to_string();
            let page = self
                .get::<OpenApiFileListResponse>(
                    self.build_api_url("/api/v2/file/list"),
                    Some(vec![
                        ("parentFileId", parent_file_id.as_str()),
                        ("limit", "100"),
                        ("lastFileId", last_file_id_value.as_str()),
                    ]),
                )
                .await?;
            let next_last_file_id = page.last_file_id;
            let raw_empty = page.file_list.is_empty();
            files.extend(page.file_list.into_iter().filter(|f| f.trashed == 0));

            if next_last_file_id <= 0 || raw_empty || next_last_file_id == last_file_id {
                break;
            }
            last_file_id = next_last_file_id;
        }

        Ok(files)
    }

    pub async fn list_dir_ids(&self, file_id: i64) -> RequestResult<HashMap<String, i64>> {
        let files = self.list(file_id).await?;
        let dir_ids = files
            .into_iter()
            .filter(|f| f.is_dir())
            .map(|f| (f.file_name.clone(), f.file_id))
            .collect::<HashMap<_, _>>();
        Ok(dir_ids)
    }

    pub async fn search(&self, file_name: &str) -> RequestResult<Vec<File>> {
        self.get::<OpenApiFileListResponse>(
            self.build_api_url("/api/v2/file/list"),
            Some(vec![
                ("parentFileId", "0"),
                ("limit", "10"),
                ("lastFileId", "0"),
                ("searchData", file_name),
            ]),
        )
        .await
        .map(|r| r.file_list.into_iter().filter(|f| f.trashed == 0).collect())
    }

    pub async fn search_dirs_with_paths(
        &self,
        file_name: &str,
    ) -> RequestResult<Vec<SearchPathFile>> {
        let search_results = self
            .search(file_name)
            .await?
            .into_iter()
            .filter(|file| file.is_dir())
            .collect::<Vec<_>>();

        let mut files = Vec::with_capacity(search_results.len());
        for file in search_results {
            let path = self.resolve_path(file.file_id).await?;
            files.push(SearchPathFile {
                file_id: file.file_id,
                file_name: file.file_name,
                is_dir: true,
                path,
            });
        }

        Ok(files)
    }

    pub async fn fast_upload(
        &self,
        parent_file_id: i64,
        file_name: &str,
        etag: &str,
        size: u64,
    ) -> RequestResult<Option<i64>> {
        self.post::<_, FastUploadResponse>(
            self.build_api_url("/upload/v2/file/create"),
            None,
            Some(&json!(
                {
                    "parentFileID": parent_file_id,
                    "filename": file_name,
                    "etag": etag,
                    "size": size,
                    "duplicate": 2,
                }
            )),
        )
        .await
        .map(|r| if r.reuse { r.file_id } else { None })
    }

    pub async fn fast_upload_with_sha1(
        &self,
        parent_file_id: i64,
        file_name: &str,
        sha1: &str,
        size: u64,
    ) -> RequestResult<Option<i64>> {
        self.post::<_, FastUploadResponse>(
            self.build_api_url("/upload/v2/file/sha1_reuse"),
            None,
            Some(&json!(
                {
                    "parentFileID": parent_file_id,
                    "filename": file_name,
                    "sha1": sha1,
                    "size": size,
                    "duplicate": 2,
                }
            )),
        )
        .await
        .map(|r| if r.reuse { r.file_id } else { None })
    }

    pub async fn mkdir(&self, parent_file_id: i64, folder_name: &str) -> RequestResult<i64> {
        self.post::<_, MkdirResponse>(
            self.build_api_url("/upload/v1/file/mkdir"),
            None,
            Some(&json!(
                {
                    "name": folder_name,
                    "parentID": parent_file_id,
                }
            )),
        )
        .await
        .map(|r| r.dir_id)
    }

    pub async fn trash_files(&self, file_ids: &[i64]) -> RequestResult<()> {
        for chunk in file_ids.chunks(100) {
            self.post::<_, TrashResponse>(
                self.build_api_url("/api/v1/file/trash"),
                None,
                Some(&json!({ "fileIDs": chunk })),
            )
            .await
            .map(|_| ())?;
        }

        Ok(())
    }

    #[allow(dead_code)]
    pub async fn mkdir_by_path(&self, folder_path: &str) -> RequestResult<i64> {
        if folder_path.is_empty() || folder_path == "/" {
            return Ok(0);
        }

        let parts = folder_path
            .split('/')
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>();
        let mut current_file_id = 0;
        for part in parts {
            match self.mkdir(current_file_id, part).await {
                Ok(dir_id) => current_file_id = dir_id,
                Err(e) => match e {
                    RequestError::AlreadyExists => {
                        current_file_id = self.get_dir_id(current_file_id, part).await?;
                    }
                    _ => return Err(e),
                },
            }
        }
        Ok(current_file_id)
    }

    #[allow(dead_code)]
    async fn get_dir_id(&self, parent_file_id: i64, folder_name: &str) -> RequestResult<i64> {
        self.list(parent_file_id)
            .await?
            .into_iter()
            .find(|f| f.is_dir() && f.file_name == folder_name)
            .map(|f| f.file_id)
            .ok_or(RequestError::NotFound(format!(
                "folder {} not found in parent {}",
                folder_name, parent_file_id
            )))
    }

    pub async fn list_share_files(
        &self,
        share_key: &str,
        share_password: &str,
        parent_file_id: i64,
    ) -> RequestResult<Vec<File>> {
        let file_id_str = parent_file_id.to_string();
        let query = Some(vec![
            ("ShareKey", share_key),
            ("SharePwd", share_password),
            ("limit", "100"),
            ("next", "-1"),
            ("orderBy", "file_name"),
            ("orderDirection", "asc"),
            ("Page", "0"),
            ("parentFileId", file_id_str.as_str()),
            ("event", "homeListFile"),
        ]);
        let response: CommonResponse<OpenApiFileListResponse> =
            http::get(self.build_web_api_url("/api/share/get"), query, None).await?;
        self.process_response(response)
            .map(|r| r.file_list.into_iter().filter(|f| f.trashed == 0).collect())
    }

    #[allow(dead_code)]
    pub async fn get_file_id_by_path(&self, path: &str) -> RequestResult<Option<i64>> {
        if path.is_empty() {
            return Ok(None);
        }

        let parts: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
        let last_part = parts.last().unwrap();
        let search_results = self.search(last_part).await?;

        'outer: for search_result in &search_results {
            if search_result.file_name != *last_part {
                continue;
            }

            let mut chain_names = vec![search_result.file_name.clone()];
            let mut current_id = match search_result.parent_file_id {
                Some(pid) => pid,
                None => {
                    if parts.len() == 1 {
                        return Ok(Some(search_result.file_id));
                    }
                    continue;
                }
            };

            for _ in 1..parts.len() {
                let details = self.mutli_get(&[current_id]).await?;
                let detail = match details.get(&current_id) {
                    Some(d) => d,
                    None => continue 'outer,
                };
                chain_names.push(detail.file_name.clone());
                match detail.parent_file_id {
                    Some(pid) if pid > 0 => current_id = pid,
                    _ => {
                        if chain_names.len() == parts.len() {
                            break;
                        }
                        continue 'outer;
                    }
                }
            }

            if chain_names.len() != parts.len() {
                continue;
            }

            chain_names.reverse();
            if chain_names.iter().zip(parts.iter()).all(|(a, b)| a == b) {
                return Ok(Some(search_result.file_id));
            }
        }
        Ok(None)
    }

    async fn mutli_get(&self, file_ids: &[i64]) -> RequestResult<HashMap<i64, FileDetail>> {
        self.post::<_, MultiGetResponse>(
            self.build_api_url("/api/v1/file/infos"),
            None,
            Some(&json!({ "fileIds": file_ids })),
        )
        .await
        .map(|r| r.file_list.into_iter().map(|f| (f.file_id, f)).collect())
    }

    async fn resolve_path(&self, file_id: i64) -> RequestResult<String> {
        let mut parts = Vec::new();
        let mut current_id = file_id;

        loop {
            let details = self.mutli_get(&[current_id]).await?;
            let detail = match details.get(&current_id) {
                Some(d) => d,
                None => break,
            };
            parts.push(detail.file_name.clone());
            match detail.parent_file_id {
                Some(pid) if pid > 0 => current_id = pid,
                _ => break,
            }
        }

        parts.reverse();
        Ok(format!("/{}", parts.join("/")))
    }

    async fn get<T: DeserializeOwned>(
        &self,
        url: String,
        query: Option<Vec<(&str, &str)>>,
    ) -> RequestResult<T> {
        let token = format!("Bearer {}", self.get_token().await?);
        let headers = Some(vec![
            (PLATFORM_KEY, PLATFORM_VALUE),
            (http::AUTH_KEY, token.as_str()),
        ]);
        let response: CommonResponse<T> = http::get(url.as_str(), query, headers).await?;
        self.process_response(response)
    }

    async fn post<P: Serialize, T: DeserializeOwned>(
        &self,
        url: String,
        query: Option<Vec<(&str, &str)>>,
        payload: Option<&P>,
    ) -> RequestResult<T> {
        let token = format!("Bearer {}", self.get_token().await?);
        let headers = Some(vec![
            (PLATFORM_KEY, PLATFORM_VALUE),
            (http::AUTH_KEY, token.as_str()),
        ]);
        let response: CommonResponse<T> = http::post(url.as_str(), query, headers, payload).await?;
        self.process_response(response)
    }

    async fn get_token(&self) -> RequestResult<String> {
        {
            let token_guard = self.token.read().await;
            if let Some(t) = token_guard.as_ref()
                && !self.is_expired(t)
            {
                return Ok(t.access_token.to_owned());
            }
        }

        let mut token_guard = self.token.write().await;
        match token_guard.as_ref() {
            Some(t) => {
                if !self.is_expired(t) {
                    return Ok(t.access_token.to_owned());
                }
            }
            None => {
                let token = self.read_token_from_cache_file(TOKEN_CACHE_FILE)?;
                if let Some(t) = token
                    && !self.is_expired(&t)
                    && !t.refresh_token.is_empty()
                {
                    *token_guard = Some(t.clone());
                    return Ok(t.access_token.to_owned());
                }
            }
        }

        let cached = self.get_access_token().await?;
        self.write_token_to_cache_file(TOKEN_CACHE_FILE, &cached)?;
        *token_guard = Some(cached.clone());
        Ok(cached.access_token.to_owned())
    }

    fn process_response<T: DeserializeOwned>(&self, resp: CommonResponse<T>) -> RequestResult<T> {
        match resp.code {
            0 => match resp.data {
                Some(d) => Ok(d),
                None => {
                    // Return empty JSON object so caller can deserialize to Option<T> or ignore
                    Ok(serde_json::from_str("{}").unwrap())
                }
            },
            1 => Err(RequestError::AlreadyExists),
            401 => Err(RequestError::Unauthorized),
            429 => Err(RequestError::TooManyRequests),
            5066 => Err(RequestError::NotFound(resp.message)),
            5103 | 5104 => Err(RequestError::ShareCancelled(resp.message)),
            _ => Err(RequestError::Other(format!(
                "api error, code: {}, message: {}",
                resp.code, resp.message
            ))),
        }
    }

    #[inline]
    fn build_api_url(&self, path: &str) -> String {
        format!("{}{}", self.open_api_base, path)
    }

    #[inline]
    fn build_web_api_url(&self, path: &str) -> String {
        format!("{}{}", WEB_API_BASE, path)
    }

    fn read_token_from_cache_file(&self, file_name: &str) -> RequestResult<Option<CachedToken>> {
        if self.cache_dir.is_empty() {
            return Err(RequestError::Other("cache dir is empty".to_string()));
        }

        let path = format!("{}/{}", self.cache_dir, file_name);
        if !Path::new(&path).exists() {
            return Ok(None);
        }

        match fs::read_to_string(&path) {
            Ok(c) => match serde_json::from_str(&c) {
                Ok(t) => Ok(Some(t)),
                Err(e) => Err(RequestError::Other(format!(
                    "deserialize token cache file [{}] failed, {}",
                    path, e
                ))),
            },
            Err(e) => Err(RequestError::Other(format!(
                "read token cache file [{}] failed, {}",
                path, e
            ))),
        }
    }

    fn write_token_to_cache_file(&self, file_name: &str, token: &CachedToken) -> RequestResult<()> {
        if self.cache_dir.is_empty() {
            return Err(RequestError::Other("cache dir is empty".to_string()));
        }

        let path = format!("{}/{}", self.cache_dir, file_name);
        if !Path::new(&self.cache_dir).exists() {
            fs::create_dir_all(&self.cache_dir).map_err(|e| {
                RequestError::Other(format!(
                    "create cache dir [{}] failed, {}",
                    self.cache_dir, e
                ))
            })?;
        }

        match serde_json::to_string(token) {
            Ok(c) => match fs::write(&path, c) {
                Ok(_) => Ok(()),
                Err(e) => Err(RequestError::Other(format!(
                    "write token to cache file [{}] failed, {}",
                    path, e
                ))),
            },
            Err(e) => Err(RequestError::Other(format!(
                "serialize token failed, {}",
                e
            ))),
        }
    }

    fn is_expired(&self, token: &CachedToken) -> bool {
        token.expired_at - time::Duration::seconds(60) <= time::OffsetDateTime::now_utc()
    }

    async fn get_access_token(&self) -> RequestResult<CachedToken> {
        let rt = self
            .read_token_from_cache_file(TOKEN_CACHE_FILE)?
            .map(|t| t.refresh_token.clone())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| self.initial_refresh_token.clone());

        let resp: RefreshTokenResp = http::get(
            &self.api_address,
            Some(vec![
                ("refresh_ui", rt.as_str()),
                ("server_use", "true"),
                ("driver_txt", "123cloud_oa"),
            ]),
            None,
        )
        .await?;

        if resp.code != 0 {
            let err = if !resp.error_description.is_empty() {
                &resp.error_description
            } else if !resp.text.is_empty() {
                &resp.text
            } else if !resp.message.is_empty() {
                &resp.message
            } else if !resp.error.is_empty() {
                &resp.error
            } else {
                "unknown error"
            };
            return Err(RequestError::Other(format!(
                "pan123 open api refresh token error, code: {}, message: {}",
                resp.code, err
            )));
        }

        if resp.access_token.is_empty() || resp.refresh_token.is_empty() {
            return Err(RequestError::Other(
                "pan123 open api refresh token error, empty access_token or refresh_token"
                    .to_string(),
            ));
        }

        if resp.expires_in <= 0 {
            return Err(RequestError::Other(format!(
                "pan123 open api refresh token error, invalid expires_in: {}",
                resp.expires_in
            )));
        }

        Ok(CachedToken {
            access_token: resp.access_token,
            refresh_token: resp.refresh_token,
            expired_at: time::OffsetDateTime::now_utc() + time::Duration::seconds(resp.expires_in),
        })
    }
}

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{header, method, path, query_param},
    };

    fn unique_cache_dir() -> String {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir()
            .join(format!("bigbrother-pan123-{nanos}"))
            .display()
            .to_string()
    }

    async fn client(server: &MockServer) -> Client {
        let client = Client::with_open_api_base(
            &format!("{}/refresh", server.uri()),
            "refresh-token",
            &unique_cache_dir(),
            server.uri().as_str(),
        );
        client
            .set_token_for_test(
                "test-token",
                time::OffsetDateTime::now_utc() + time::Duration::hours(1),
            )
            .await;
        client
    }

    fn file_json(
        file_id: i64,
        file_name: &str,
        file_type: i32,
        parent_file_id: i64,
    ) -> serde_json::Value {
        serde_json::json!({
            "fileId": file_id,
            "filename": file_name,
            "type": file_type,
            "size": 1234,
            "etag": format!("etag-{file_id}"),
            "parentFileId": parent_file_id,
        })
    }

    #[test]
    fn process_response_maps_common_error_codes() {
        let client = Client::new(
            "http://api.test/refresh",
            "refresh-token",
            "/tmp/pan123-tests",
        );

        assert!(matches!(
            client.process_response::<serde_json::Value>(CommonResponse {
                code: 1,
                message: "exists".to_string(),
                data: None,
            }),
            Err(RequestError::AlreadyExists)
        ));
        assert!(matches!(
            client.process_response::<serde_json::Value>(CommonResponse {
                code: 401,
                message: "unauthorized".to_string(),
                data: None,
            }),
            Err(RequestError::Unauthorized)
        ));
        assert!(matches!(
            client.process_response::<serde_json::Value>(CommonResponse {
                code: 429,
                message: "busy".to_string(),
                data: None,
            }),
            Err(RequestError::TooManyRequests)
        ));
        assert!(matches!(
            client.process_response::<serde_json::Value>(CommonResponse {
                code: 5066,
                message: "missing".to_string(),
                data: None,
            }),
            Err(RequestError::NotFound(message)) if message == "missing"
        ));
        assert!(matches!(
            client.process_response::<serde_json::Value>(CommonResponse {
                code: 5103,
                message: "此分享不存在".to_string(),
                data: None,
            }),
            Err(RequestError::ShareCancelled(message)) if message == "此分享不存在"
        ));
        assert!(matches!(
            client.process_response::<serde_json::Value>(CommonResponse {
                code: 5104,
                message: "分享已过期".to_string(),
                data: None,
            }),
            Err(RequestError::ShareCancelled(message)) if message == "分享已过期"
        ));
    }

    #[test]
    fn write_then_read_token_cache_file_round_trips() {
        let cache_dir = unique_cache_dir();
        let client = Client::new("http://api.test/refresh", "refresh-token", &cache_dir);
        let token = CachedToken {
            access_token: "cached-token".to_string(),
            refresh_token: "cached-refresh".to_string(),
            expired_at: time::OffsetDateTime::now_utc() + time::Duration::hours(1),
        };

        client
            .write_token_to_cache_file(TOKEN_CACHE_FILE, &token)
            .unwrap();
        let loaded = client
            .read_token_from_cache_file(TOKEN_CACHE_FILE)
            .unwrap()
            .unwrap();

        assert_eq!(loaded.access_token, "cached-token");
        assert_eq!(loaded.refresh_token, "cached-refresh");

        let _ = fs::remove_dir_all(cache_dir);
    }

    #[tokio::test]
    async fn list_dir_ids_returns_only_directories() {
        let server = MockServer::start().await;
        let client = client(&server).await;

        Mock::given(method("GET"))
            .and(path("/api/v2/file/list"))
            .and(query_param("parentFileId", "42"))
            .and(header("Platform", "open_platform"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "code": 0,
                "message": "ok",
                "data": {
                    "lastFileId": -1,
                    "fileList": [
                        file_json(10, "Season 1", 1, 42),
                        file_json(11, "episode.mkv", 0, 42)
                    ]
                }
            })))
            .mount(&server)
            .await;

        let result = client.list_dir_ids(42).await.unwrap();

        assert_eq!(result.len(), 1);
        assert_eq!(result.get("Season 1"), Some(&10));
    }

    #[tokio::test]
    async fn list_paginates_until_last_file_id_is_terminal() {
        let server = MockServer::start().await;
        let client = client(&server).await;

        Mock::given(method("GET"))
            .and(path("/api/v2/file/list"))
            .and(query_param("parentFileId", "7"))
            .and(query_param("lastFileId", "0"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "code": 0,
                "message": "ok",
                "data": {
                    "lastFileId": 50,
                    "fileList": [file_json(10, "first", 1, 7)]
                }
            })))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/api/v2/file/list"))
            .and(query_param("parentFileId", "7"))
            .and(query_param("lastFileId", "50"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "code": 0,
                "message": "ok",
                "data": {
                    "lastFileId": -1,
                    "fileList": [file_json(11, "second", 0, 7)]
                }
            })))
            .expect(1)
            .mount(&server)
            .await;

        let result = client.list(7).await.unwrap();

        assert_eq!(result.len(), 2);
        assert_eq!(result[0].file_name, "first");
        assert_eq!(result[1].file_name, "second");
    }

    #[tokio::test]
    async fn list_stops_when_last_file_id_is_omitted() {
        let server = MockServer::start().await;
        let client = client(&server).await;

        Mock::given(method("GET"))
            .and(path("/api/v2/file/list"))
            .and(query_param("parentFileId", "8"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "code": 0,
                "message": "ok",
                "data": {
                    "fileList": [file_json(20, "only", 1, 8)]
                }
            })))
            .expect(1)
            .mount(&server)
            .await;

        let result = client.list(8).await.unwrap();

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].file_id, 20);
    }

    #[tokio::test]
    async fn get_download_url_returns_url() {
        let server = MockServer::start().await;
        let client = client(&server).await;

        Mock::given(method("GET"))
            .and(path("/api/v1/file/download_info"))
            .and(query_param("fileId", "99"))
            .and(header("Platform", "open_platform"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "code": 0,
                "message": "ok",
                "data": {
                    "downloadUrl": "https://cdn.example.com/download/movie.mkv"
                }
            })))
            .mount(&server)
            .await;

        let result = client.get_download_url(99).await.unwrap();

        assert_eq!(result, "https://cdn.example.com/download/movie.mkv");
    }

    #[tokio::test]
    async fn get_download_url_returns_error_when_url_is_empty() {
        let server = MockServer::start().await;
        let client = client(&server).await;

        Mock::given(method("GET"))
            .and(path("/api/v1/file/download_info"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "code": 0,
                "message": "ok",
                "data": {
                    "downloadUrl": ""
                }
            })))
            .mount(&server)
            .await;

        let error = client.get_download_url(99).await.unwrap_err();

        match error {
            RequestError::Other(message) => {
                assert!(message.contains("empty download url"));
            }
            other => panic!("expected request error, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn get_file_id_by_path_returns_matching_file_id() {
        let server = MockServer::start().await;
        let client = client(&server).await;

        Mock::given(method("GET"))
            .and(path("/api/v2/file/list"))
            .and(query_param("searchData", "movie.mkv"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "code": 0,
                "message": "ok",
                "data": {
                    "lastFileId": -1,
                    "fileList": [
                        file_json(30, "movie.mkv", 0, 20)
                    ]
                }
            })))
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/api/v1/file/infos"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "code": 0,
                "message": "ok",
                "data": {
                    "fileList": [
                        {"fileId": 20, "filename": "Season 1", "size": 0, "etag": "e20", "parentFileID": 10},
                        {"fileId": 10, "filename": "Shows", "size": 0, "etag": "e10", "parentFileID": 0}
                    ]
                }
            })))
            .mount(&server)
            .await;

        let result = client
            .get_file_id_by_path("/Shows/Season 1/movie.mkv")
            .await
            .unwrap();

        assert_eq!(result, Some(30));
    }

    #[tokio::test]
    async fn search_dirs_with_paths_returns_resolved_human_paths() {
        let server = MockServer::start().await;
        let client = client(&server).await;

        Mock::given(method("GET"))
            .and(path("/api/v2/file/list"))
            .and(query_param("searchData", "breaking"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "code": 0,
                "message": "ok",
                "data": {
                    "lastFileId": -1,
                    "fileList": [
                        file_json(30, "Breaking Bad (2008) {tmdb-1396}", 1, 20)
                    ]
                }
            })))
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/api/v1/file/infos"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "code": 0,
                "message": "ok",
                "data": {
                    "fileList": [
                        {"fileId": 30, "filename": "Breaking Bad (2008) {tmdb-1396}", "size": 0, "etag": "e30", "parentFileID": 20},
                        {"fileId": 20, "filename": "电视剧", "size": 0, "etag": "e20", "parentFileID": 10},
                        {"fileId": 10, "filename": "remote", "size": 0, "etag": "e10", "parentFileID": 0}
                    ]
                }
            })))
            .mount(&server)
            .await;

        let result = client.search_dirs_with_paths("breaking").await.unwrap();

        assert_eq!(
            result,
            vec![SearchPathFile {
                file_id: 30,
                file_name: "Breaking Bad (2008) {tmdb-1396}".to_string(),
                is_dir: true,
                path: "/remote/电视剧/Breaking Bad (2008) {tmdb-1396}".to_string(),
            }]
        );
    }

    #[tokio::test]
    async fn search_dirs_with_paths_skips_non_dirs() {
        let server = MockServer::start().await;
        let client = client(&server).await;

        Mock::given(method("GET"))
            .and(path("/api/v2/file/list"))
            .and(query_param("searchData", "breaking"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "code": 0,
                "message": "ok",
                "data": {
                    "lastFileId": -1,
                    "fileList": [
                        file_json(30, "Breaking Bad (2008) {tmdb-1396}", 1, 20),
                        file_json(31, "Breaking Bad (2008) {tmdb-1396}.mkv", 0, 20)
                    ]
                }
            })))
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/api/v1/file/infos"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "code": 0,
                "message": "ok",
                "data": {
                    "fileList": [
                        {"fileId": 30, "filename": "Breaking Bad (2008) {tmdb-1396}", "size": 0, "etag": "e30", "parentFileID": 20},
                        {"fileId": 20, "filename": "电视剧", "size": 0, "etag": "e20", "parentFileID": 10},
                        {"fileId": 10, "filename": "remote", "size": 0, "etag": "e10", "parentFileID": 0}
                    ]
                }
            })))
            .mount(&server)
            .await;

        let result = client.search_dirs_with_paths("breaking").await.unwrap();

        assert_eq!(result.len(), 1);
        assert!(result[0].is_dir);
        assert_eq!(result[0].file_id, 30);
    }
}
