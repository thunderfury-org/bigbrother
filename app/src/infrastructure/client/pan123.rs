use std::{collections::HashMap, fs, path::Path, sync::Arc};

use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::json;
use tokio::sync::RwLock;

use super::{RequestError, RequestResult, http};

const API_BASE: &str = "https://www.123pan.com/b";

const APP_VERSION_KEY: &str = "App-Version";
const APP_VERSION_VALUE: &str = "3";
const PLATFORM_KEY: &str = "Platform";
const PLATFORM_VALUE: &str = "web";
const REFERER_KEY: &str = "Referer";
const REFERER_VALUE: &str = "https://www.123pan.com/";

const TOKEN_CACHE_FILE: &str = "token.json";

#[derive(Debug, Deserialize)]
struct CommonResponse<T> {
    code: i32,
    message: String,
    data: Option<T>,
}

#[derive(Debug, Deserialize)]
pub struct File {
    #[serde(rename = "FileId")]
    pub file_id: i64,
    #[serde(rename = "FileName")]
    pub file_name: String,
    /// 0: file, 1: folder
    #[serde(rename = "Type")]
    pub file_type: i32,
    #[serde(rename = "Size")]
    pub size: u64,
    #[serde(rename = "CreateAt", with = "time::serde::rfc3339")]
    pub _created_at: time::OffsetDateTime,
    #[serde(rename = "UpdateAt", with = "time::serde::rfc3339")]
    pub _updated_at: time::OffsetDateTime,
    #[serde(rename = "Etag")]
    pub etag: String,
    #[serde(default, rename = "AbsPath")]
    pub abs_path: String,
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
struct FileListResponse {
    #[serde(rename = "Next")]
    pub _next: String,
    #[serde(rename = "Len")]
    pub _len: i32,
    #[serde(rename = "IsFirst")]
    pub _is_first: bool,
    #[serde(rename = "InfoList")]
    pub info_list: Vec<File>,
}

#[derive(Debug, Deserialize)]
struct FastUploadResponse {
    #[serde(rename = "Reuse")]
    reuse: bool,
    #[serde(rename = "Info")]
    info: Option<File>,
}

#[derive(Debug, Deserialize)]
struct FastUploadWithSha1Response {
    #[serde(rename = "reuse")]
    reuse: bool,
    #[serde(rename = "fileID")]
    file_id: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct TrashResponse {}

#[derive(Debug, Deserialize)]
pub struct FileDetail {
    #[serde(rename = "FileId")]
    pub file_id: i64,
    #[serde(rename = "FileName")]
    pub file_name: String,
    #[serde(rename = "Size")]
    pub size: u64,
    #[serde(rename = "Etag")]
    pub etag: String,
    #[serde(rename = "S3KeyFlag")]
    pub s3_key_flag: String,
}

#[derive(Debug, Deserialize)]
struct MultiGetResponse {
    #[serde(rename = "infoList")]
    file_list: Vec<FileDetail>,
}

#[derive(Debug, Deserialize)]
struct DownloadDispatch {
    #[serde(rename = "prefix")]
    prefix: String,
}

#[derive(Debug, Deserialize)]
struct DownloadInfo {
    #[serde(rename = "downloadPath")]
    download_path: String,
    #[serde(rename = "dispatchList")]
    dispatch_list: Vec<DownloadDispatch>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AccessToken {
    #[serde(rename = "token")]
    token: String,
    #[serde(rename = "expire", with = "time::serde::rfc3339")]
    expired_at: time::OffsetDateTime,
}

#[derive(Debug, Default, Clone)]
pub struct Client {
    passport: String,
    password: String,
    host: String,
    cache_dir: String,
    token: Arc<RwLock<Option<AccessToken>>>,
}

impl Client {
    pub fn new(passport: &str, password: &str, cache_dir: &str) -> Self {
        Self {
            passport: passport.to_owned(),
            password: password.to_owned(),
            host: API_BASE.to_owned(),
            cache_dir: cache_dir.to_owned(),
            token: Arc::new(RwLock::new(None)),
        }
    }

    #[cfg(test)]
    pub(crate) fn with_host(passport: &str, password: &str, cache_dir: &str, host: &str) -> Self {
        Self {
            passport: passport.to_owned(),
            password: password.to_owned(),
            host: host.to_owned(),
            cache_dir: cache_dir.to_owned(),
            token: Arc::new(RwLock::new(None)),
        }
    }

    #[cfg(test)]
    pub(crate) async fn set_token_for_test(&self, token: &str, expired_at: time::OffsetDateTime) {
        let mut guard = self.token.write().await;
        *guard = Some(AccessToken {
            token: token.to_owned(),
            expired_at,
        });
    }

    pub async fn get_download_url(&self, file_id: i64) -> RequestResult<String> {
        let files = self.mutli_get(&[file_id]).await?;
        match files.get(&file_id) {
            Some(f) => {
                let download_info = self
                    .post::<_, DownloadInfo>(
                        self.build_api_url("/api/v2/file/download_info"),
                        None,
                        Some(&json!(
                            {
                                "driveId": 0,
                                "fileId": file_id,
                                "etag": f.etag,
                                "size": f.size,
                                "s3keyFlag": f.s3_key_flag,
                                "fileName": f.file_name,
                                "type": 0,
                            }
                        )),
                    )
                    .await?;
                if download_info.dispatch_list.is_empty() {
                    Err(RequestError::Error(
                        "get download url failed, no dispatch available".to_string(),
                    ))
                } else {
                    Ok(format!(
                        "{}{}",
                        download_info.dispatch_list[0].prefix, download_info.download_path
                    ))
                }
            }
            None => Err(RequestError::NotFound(format!(
                "file {} not found",
                file_id
            ))),
        }
    }

    pub async fn download_file(&self, file_id: i64, local_file_path: &str) -> RequestResult<()> {
        let download_url = self.get_download_url(file_id).await?;
        http::download_file(download_url.as_str(), local_file_path).await
    }

    pub async fn list(&self, file_id: i64) -> RequestResult<Vec<File>> {
        let parent_file_id = file_id.to_string();
        self.get::<FileListResponse>(
            self.build_api_url("/api/file/list/new"),
            Some(vec![
                ("driveId", "0"),
                ("limit", "100"),
                ("next", "0"),
                ("orderBy", "file_name"),
                ("orderDirection", "asc"),
                ("parentFileId", parent_file_id.as_str()),
                ("trashed", "false"),
                ("SearchData", ""),
                ("Page", "1"),
                ("OnlyLookAbnormalFile", "0"),
                ("event", "homeListFile"),
                ("operateType", "1"),
                ("inDirectSpace", "false"),
                ("fileCategory", "0"),
                ("isSearchOrder", "false"),
            ]),
        )
        .await
        .map(|r| r.info_list)
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
        self.get::<FileListResponse>(
            self.build_api_url("/api/file/list/new"),
            Some(vec![
                ("driveId", "0"),
                ("limit", "10"),
                ("next", "0"),
                ("orderDirection", "desc"),
                ("parentFileId", "0"),
                ("trashed", "false"),
                ("SearchData", file_name),
                ("Page", "1"),
                ("OnlyLookAbnormalFile", "0"),
                ("event", "homeListFile"),
                ("operateType", "2"),
            ]),
        )
        .await
        .map(|r| r.info_list)
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
        let file_ids = search_results
            .iter()
            .flat_map(|file| file.abs_path.split('/'))
            .filter_map(|segment| segment.parse::<i64>().ok())
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        let file_details = if file_ids.is_empty() {
            HashMap::new()
        } else {
            self.mutli_get(file_ids.as_slice()).await?
        };
        let mut files = Vec::with_capacity(search_results.len());

        for file in search_results {
            let Some(path) = resolve_path_from_details(file.abs_path.as_str(), &file_details)
            else {
                continue;
            };
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
            self.build_api_url("/api/file/upload_request"),
            None,
            Some(&json!(
                {
                    "driveId": 0,
                    "parentFileId": parent_file_id,
                    "fileName": file_name,
                    "etag": etag,
                    "size": size,
                    "type": 0,
                    "duplicate": 2,
                }
            )),
        )
        .await
        .map(|r| {
            if r.reuse {
                match r.info {
                    Some(info) => Some(info.file_id),
                    None => None,
                }
            } else {
                None
            }
        })
    }

    /// 需要 openapi 才能使用
    pub async fn fast_upload_with_sha1(
        &self,
        parent_file_id: i64,
        file_name: &str,
        sha1: &str,
        size: u64,
    ) -> RequestResult<Option<i64>> {
        self.post::<_, FastUploadWithSha1Response>(
            // 这里需要改成 openapi 的地址
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
        self.post::<_, FastUploadResponse>(
            self.build_api_url("/api/file/upload_request"),
            None,
            Some(&json!(
                {
                    "driveId": 0,
                    "parentFileId": parent_file_id,
                    "fileName": folder_name,
                    "etag": "",
                    "size": 0,
                    "type": 1,
                    "duplicate": 2,
                    "NotReuse": false,
                }
            )),
        )
        .await
        .map(|r| match r.info {
            Some(info) => info.file_id,
            None => 0,
        })
    }

    pub async fn trash_files(&self, file_ids: &[i64]) -> RequestResult<()> {
        for chunk in file_ids.chunks(100) {
            let files = chunk
                .iter()
                .map(|id| json!({"FileId": id}))
                .collect::<Vec<_>>();
            self.post::<_, TrashResponse>(
                self.build_api_url("/api/file/trash"),
                None,
                Some(&json!(
                    {
                        "driveId": 0,
                        "event": "intoRecycle",
                        "fileTrashInfoList": files,
                        "operatePlace": 1,
                        "operation": true,
                        "safeBox": false,
                    }
                )),
            )
            .await
            .map(|_| ())?;
        }

        Ok(())
    }

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
        let response: CommonResponse<FileListResponse> =
            http::get(self.build_api_url("/api/share/get"), query, None).await?;
        self.process_response(response).map(|r| r.info_list)
    }

    pub async fn get_file_id_by_path(&self, path: &str) -> RequestResult<Option<i64>> {
        if path.is_empty() {
            return Ok(None);
        }

        let parts = path
            .split('/')
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>();
        let last_part = parts.last().unwrap();
        let search_results = self.search(last_part).await?;

        for search_result in &search_results {
            if search_result.file_name != *last_part {
                continue;
            }

            let file_ids = search_result
                .abs_path
                .split('/')
                .filter_map(|s| s.parse::<i64>().ok())
                .collect::<Vec<_>>();
            if file_ids.len() != parts.len() {
                continue;
            }

            let files = self.mutli_get(&file_ids).await?;
            if files.len() != file_ids.len() {
                continue;
            }

            let mut all_match = true;
            for i in 0..file_ids.len() {
                if files.get(&file_ids[i]).unwrap().file_name != parts[i] {
                    all_match = false;
                    break;
                }
            }
            if all_match {
                return Ok(Some(search_result.file_id));
            }
        }
        Ok(None)
    }

    async fn mutli_get(&self, file_ids: &[i64]) -> RequestResult<HashMap<i64, FileDetail>> {
        let files = file_ids
            .iter()
            .map(|id| json!({"FileId": id}))
            .collect::<Vec<_>>();
        self.post::<_, MultiGetResponse>(
            self.build_api_url("/api/file/info"),
            None,
            Some(&json!(
                {
                    "fileIdList": files,
                }
            )),
        )
        .await
        .map(|r| r.file_list.into_iter().map(|f| (f.file_id, f)).collect())
    }

    async fn get<T: DeserializeOwned>(
        &self,
        url: String,
        query: Option<Vec<(&str, &str)>>,
    ) -> RequestResult<T> {
        let token = format!("Bearer {}", self.get_token().await?);
        let headers = Some(vec![
            (APP_VERSION_KEY, APP_VERSION_VALUE),
            (PLATFORM_KEY, PLATFORM_VALUE),
            (REFERER_KEY, REFERER_VALUE),
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
            (APP_VERSION_KEY, APP_VERSION_VALUE),
            (PLATFORM_KEY, PLATFORM_VALUE),
            (REFERER_KEY, REFERER_VALUE),
            (http::AUTH_KEY, token.as_str()),
        ]);
        let response: CommonResponse<T> = http::post(url.as_str(), query, headers, payload).await?;
        self.process_response(response)
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
            _ => Err(RequestError::Error(format!(
                "api error, code: {}, message: {}",
                resp.code, resp.message
            ))),
        }
    }

    #[inline]
    fn build_api_url(&self, path: &str) -> String {
        format!("{}{}", self.host, path)
    }

    async fn get_token(&self) -> RequestResult<String> {
        // --- 第一次检查（无锁）---
        {
            let token_guard = self.token.read().await;
            if let Some(t) = token_guard.as_ref()
                && !self.is_expired(t)
            {
                // 缓存有效，快速返回（并发读）
                return Ok(t.token.to_owned());
            }
            // 读锁在此作用域结束时自动释放
            // 没有缓存或者缓存过期，需要刷新
        }

        // --- 第二次检查和操作（持有写锁）---
        // 只有在第一次检查失败时，才竞争写锁
        let mut token_guard = self.token.write().await;
        match token_guard.as_ref() {
            Some(t) => {
                if !self.is_expired(t) {
                    // 在我们等待写锁时，另一个任务可能已经完成了刷新
                    return Ok(t.token.to_owned());
                }

                // 缓存过期，需要刷新
            }
            None => {
                // 没有缓存，尝试从缓存文件读取
                let token = self.read_token_from_cache_file()?;
                if let Some(t) = token
                    && !self.is_expired(&t)
                {
                    // 缓存文件中的token有效，快速返回（并发读）
                    *token_guard = Some(t.clone());
                    return Ok(t.token.to_owned());
                }

                // 缓存文件不存在或者过期，需要刷新
            }
        }

        // 真正需要刷新：执行网络操作
        let access_token = self.get_access_token().await?;

        // 写入缓存文件
        self.write_token_to_cache_file(&access_token)?;
        // 更新缓存
        *token_guard = Some(access_token.clone());

        // 写锁在此作用域结束时自动释放
        Ok(access_token.token.to_owned())
    }

    fn read_token_from_cache_file(&self) -> RequestResult<Option<AccessToken>> {
        if self.cache_dir.is_empty() {
            return Err(RequestError::Error("cache dir is empty".to_string()));
        }

        let path = format!("{}/{}", self.cache_dir, TOKEN_CACHE_FILE);
        if !Path::new(&path).exists() {
            return Ok(None);
        }

        match fs::read_to_string(&path) {
            Ok(c) => match serde_json::from_str(&c) {
                Ok(t) => Ok(Some(t)),
                Err(e) => Err(RequestError::Error(format!(
                    "deserialize token cache file [{}] failed, {}",
                    path, e
                ))),
            },
            Err(e) => Err(RequestError::Error(format!(
                "read token cache file [{}] failed, {}",
                path, e
            ))),
        }
    }

    fn write_token_to_cache_file(&self, token: &AccessToken) -> RequestResult<()> {
        if self.cache_dir.is_empty() {
            return Err(RequestError::Error("cache dir is empty".to_string()));
        }

        let path = format!("{}/{}", self.cache_dir, TOKEN_CACHE_FILE);
        if !Path::new(&self.cache_dir).exists() {
            fs::create_dir_all(&self.cache_dir).map_err(|e| {
                RequestError::Error(format!(
                    "create cache dir [{}] failed, {}",
                    self.cache_dir, e
                ))
            })?;
        }

        match serde_json::to_string(token) {
            Ok(c) => match fs::write(&path, c) {
                Ok(_) => Ok(()),
                Err(e) => Err(RequestError::Error(format!(
                    "write token to cache file [{}] failed, {}",
                    path, e
                ))),
            },
            Err(e) => Err(RequestError::Error(format!(
                "serialize token failed, {}",
                e
            ))),
        }
    }

    fn is_expired(&self, token: &AccessToken) -> bool {
        token.expired_at - time::Duration::seconds(60) <= time::OffsetDateTime::now_utc()
    }

    async fn get_access_token(&self) -> RequestResult<AccessToken> {
        let response: CommonResponse<AccessToken> = http::post(
            self.build_api_url("/api/user/sign_in"),
            None,
            Some(vec![
                (APP_VERSION_KEY, APP_VERSION_VALUE),
                (PLATFORM_KEY, PLATFORM_VALUE),
                (REFERER_KEY, REFERER_VALUE),
            ]),
            Some(&json!({
                "passport": self.passport,
                "password": self.password,
                "remember": true,
            })),
        )
        .await?;

        match response.code {
            200 => match response.data {
                Some(d) => Ok(d),
                None => Err(RequestError::Error(format!(
                    "pan123 sign_in error, data is empty, code: {}, message: {}",
                    response.code, response.message
                ))),
            },
            _ => Err(RequestError::Error(format!(
                "pan123 sign_in error, code: {}, message: {}",
                response.code, response.message
            ))),
        }
    }
}

fn resolve_path_from_details(abs_path: &str, files: &HashMap<i64, FileDetail>) -> Option<String> {
    let file_ids = abs_path
        .split('/')
        .filter_map(|s| s.parse::<i64>().ok())
        .collect::<Vec<_>>();
    if file_ids.is_empty() {
        return None;
    }

    let mut parts = Vec::with_capacity(file_ids.len());
    for file_id in file_ids {
        let file = files.get(&file_id)?;
        parts.push(file.file_name.clone());
    }

    Some(format!("/{}", parts.join("/")))
}

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{body_json, header, method, path, query_param},
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
        let client = Client::with_host("user", "pass", &unique_cache_dir(), server.uri().as_str());
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
        abs_path: &str,
    ) -> serde_json::Value {
        serde_json::json!({
            "FileId": file_id,
            "FileName": file_name,
            "Type": file_type,
            "Size": 1234,
            "CreateAt": "2024-01-01T00:00:00Z",
            "UpdateAt": "2024-01-01T00:00:00Z",
            "Etag": format!("etag-{file_id}"),
            "AbsPath": abs_path,
        })
    }

    #[test]
    fn process_response_maps_common_error_codes() {
        let client = Client::new("user", "pass", "/tmp/pan123-tests");

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
    }

    #[test]
    fn write_then_read_token_cache_file_round_trips() {
        let cache_dir = unique_cache_dir();
        let client = Client::new("user", "pass", &cache_dir);
        let token = AccessToken {
            token: "cached-token".to_string(),
            expired_at: time::OffsetDateTime::now_utc() + time::Duration::hours(1),
        };

        client.write_token_to_cache_file(&token).unwrap();
        let loaded = client.read_token_from_cache_file().unwrap().unwrap();

        assert_eq!(loaded.token, "cached-token");

        let _ = fs::remove_dir_all(cache_dir);
    }

    #[tokio::test]
    async fn list_dir_ids_returns_only_directories() {
        let server = MockServer::start().await;
        let client = client(&server).await;

        Mock::given(method("GET"))
            .and(path("/api/file/list/new"))
            .and(query_param("parentFileId", "42"))
            .and(query_param("operateType", "1"))
            .and(header("authorization", "Bearer test-token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "code": 0,
                "message": "ok",
                "data": {
                    "Next": "0",
                    "Len": 2,
                    "IsFirst": true,
                    "InfoList": [
                        file_json(10, "Season 1", 1, "/10"),
                        file_json(11, "episode.mkv", 0, "/11")
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
    async fn get_download_url_returns_prefixed_path() {
        let server = MockServer::start().await;
        let client = client(&server).await;

        Mock::given(method("POST"))
            .and(path("/api/file/info"))
            .and(header("authorization", "Bearer test-token"))
            .and(body_json(serde_json::json!({
                "fileIdList": [{"FileId": 99}]
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "code": 0,
                "message": "ok",
                "data": {
                    "infoList": [{
                        "FileId": 99,
                        "FileName": "movie.mkv",
                        "Size": 2048,
                        "Etag": "etag-99",
                        "S3KeyFlag": "flag-99"
                    }]
                }
            })))
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/api/v2/file/download_info"))
            .and(header("authorization", "Bearer test-token"))
            .and(body_json(serde_json::json!({
                "driveId": 0,
                "fileId": 99,
                "etag": "etag-99",
                "size": 2048,
                "s3keyFlag": "flag-99",
                "fileName": "movie.mkv",
                "type": 0
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "code": 0,
                "message": "ok",
                "data": {
                    "downloadPath": "/download/movie.mkv",
                    "dispatchList": [{"prefix": "https://cdn.example.com"}]
                }
            })))
            .mount(&server)
            .await;

        let result = client.get_download_url(99).await.unwrap();

        assert_eq!(result, "https://cdn.example.com/download/movie.mkv");
    }

    #[tokio::test]
    async fn get_download_url_returns_not_found_when_multi_get_misses() {
        let server = MockServer::start().await;
        let client = client(&server).await;

        Mock::given(method("POST"))
            .and(path("/api/file/info"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "code": 0,
                "message": "ok",
                "data": {
                    "infoList": []
                }
            })))
            .mount(&server)
            .await;

        let error = client.get_download_url(99).await.unwrap_err();

        match error {
            RequestError::NotFound(message) => assert!(message.contains("file 99 not found")),
            other => panic!("expected not found, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn get_download_url_returns_error_when_dispatch_is_empty() {
        let server = MockServer::start().await;
        let client = client(&server).await;

        Mock::given(method("POST"))
            .and(path("/api/file/info"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "code": 0,
                "message": "ok",
                "data": {
                    "infoList": [{
                        "FileId": 99,
                        "FileName": "movie.mkv",
                        "Size": 2048,
                        "Etag": "etag-99",
                        "S3KeyFlag": "flag-99"
                    }]
                }
            })))
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/api/v2/file/download_info"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "code": 0,
                "message": "ok",
                "data": {
                    "downloadPath": "/download/movie.mkv",
                    "dispatchList": []
                }
            })))
            .mount(&server)
            .await;

        let error = client.get_download_url(99).await.unwrap_err();

        match error {
            RequestError::Error(message) => {
                assert!(message.contains("no dispatch available"));
            }
            other => panic!("expected request error, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn get_file_id_by_path_returns_matching_file_id() {
        let server = MockServer::start().await;
        let client = client(&server).await;

        Mock::given(method("GET"))
            .and(path("/api/file/list/new"))
            .and(query_param("SearchData", "movie.mkv"))
            .and(query_param("operateType", "2"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "code": 0,
                "message": "ok",
                "data": {
                    "Next": "0",
                    "Len": 1,
                    "IsFirst": true,
                    "InfoList": [
                        file_json(30, "movie.mkv", 0, "/10/20/30")
                    ]
                }
            })))
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/api/file/info"))
            .and(body_json(serde_json::json!({
                "fileIdList": [
                    {"FileId": 10},
                    {"FileId": 20},
                    {"FileId": 30}
                ]
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "code": 0,
                "message": "ok",
                "data": {
                    "infoList": [
                        {"FileId": 10, "FileName": "Shows", "Size": 0, "Etag": "e10", "S3KeyFlag": "s10"},
                        {"FileId": 20, "FileName": "Season 1", "Size": 0, "Etag": "e20", "S3KeyFlag": "s20"},
                        {"FileId": 30, "FileName": "movie.mkv", "Size": 1234, "Etag": "e30", "S3KeyFlag": "s30"}
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
            .and(path("/api/file/list/new"))
            .and(query_param("SearchData", "breaking"))
            .and(query_param("operateType", "2"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "code": 0,
                "message": "ok",
                "data": {
                    "Next": "0",
                    "Len": 1,
                    "IsFirst": true,
                    "InfoList": [
                        file_json(30, "Breaking Bad (2008) {tmdb-1396}", 1, "/10/20/30")
                    ]
                }
            })))
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/api/file/info"))
            .and(body_json(serde_json::json!({
                "fileIdList": [
                    {"FileId": 10},
                    {"FileId": 20},
                    {"FileId": 30}
                ]
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "code": 0,
                "message": "ok",
                "data": {
                    "infoList": [
                        {"FileId": 10, "FileName": "remote", "Size": 0, "Etag": "e10", "S3KeyFlag": "s10"},
                        {"FileId": 20, "FileName": "电视剧", "Size": 0, "Etag": "e20", "S3KeyFlag": "s20"},
                        {"FileId": 30, "FileName": "Breaking Bad (2008) {tmdb-1396}", "Size": 0, "Etag": "e30", "S3KeyFlag": "s30"}
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
    async fn search_dirs_with_paths_skips_non_dirs_before_resolving_paths() {
        let server = MockServer::start().await;
        let client = client(&server).await;

        Mock::given(method("GET"))
            .and(path("/api/file/list/new"))
            .and(query_param("SearchData", "breaking"))
            .and(query_param("operateType", "2"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "code": 0,
                "message": "ok",
                "data": {
                    "Next": "0",
                    "Len": 2,
                    "IsFirst": true,
                    "InfoList": [
                        file_json(30, "Breaking Bad (2008) {tmdb-1396}", 1, "/10/20/30"),
                        file_json(31, "Breaking Bad (2008) {tmdb-1396}.mkv", 0, "/10/20/31")
                    ]
                }
            })))
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/api/file/info"))
            .and(body_json(serde_json::json!({
                "fileIdList": [
                    {"FileId": 10},
                    {"FileId": 20},
                    {"FileId": 30}
                ]
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "code": 0,
                "message": "ok",
                "data": {
                    "infoList": [
                        {"FileId": 10, "FileName": "remote", "Size": 0, "Etag": "e10", "S3KeyFlag": "s10"},
                        {"FileId": 20, "FileName": "电视剧", "Size": 0, "Etag": "e20", "S3KeyFlag": "s20"},
                        {"FileId": 30, "FileName": "Breaking Bad (2008) {tmdb-1396}", "Size": 0, "Etag": "e30", "S3KeyFlag": "s30"}
                    ]
                }
            })))
            .expect(1)
            .mount(&server)
            .await;

        let result = client.search_dirs_with_paths("breaking").await.unwrap();

        assert_eq!(result.len(), 1);
        assert!(result[0].is_dir);
        assert_eq!(result[0].file_id, 30);
    }
}
