use std::{fs, path::Path, sync::Arc};

use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::json;
use tokio::sync::RwLock;

use super::{RequestError, RequestResult};

const API_BASE: &str = "https://www.123pan.com/b";
const OPEN_API_BASE: &str = "https://open-api.123pan.com";
const PLATFORM_KEY: &str = "Platform";
const PLATFORM_VALUE: &str = "open_platform";
const UA_KEY: &str = "User-Agent";
const UA_VALUE: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/141.0.0.0 Safari/537.36";
const AUTH_KEY: &str = "Authorization";

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
    #[serde(rename = "S3KeyFlag")]
    pub s3_key_flag: String,
    #[serde(rename = "CreateAt", with = "time::serde::rfc3339")]
    pub created_at: time::OffsetDateTime,
    #[serde(rename = "UpdateAt", with = "time::serde::rfc3339")]
    pub updated_at: time::OffsetDateTime,
    #[serde(rename = "Etag")]
    pub etag: String,
    #[serde(rename = "ParentFileId")]
    pub parent_file_id: i64,
    #[serde(rename = "DownloadUrl")]
    pub download_url: String,
    #[serde(rename = "Trashed")]
    pub trashed: bool,
    #[serde(rename = "AbsPath")]
    pub abs_path: String,
    #[serde(rename = "NewParentName")]
    pub new_parent_name: String,
}

#[derive(Debug, Deserialize)]
struct FileListResponse {
    #[serde(rename = "Next")]
    pub next: String,
    #[serde(rename = "Len")]
    pub len: i32,
    #[serde(rename = "IsFirst")]
    pub is_first: bool,
    #[serde(rename = "Total")]
    pub total: i32,
    #[serde(rename = "SearchFileDesc")]
    pub search_file_desc: String,
    #[serde(rename = "InfoList")]
    pub info_list: Vec<File>,
}

#[derive(Debug, Deserialize)]
struct FastUploadResponse {
    #[serde(rename = "fileID")]
    file_id: Option<i64>,
    #[serde(rename = "reuse")]
    reuse: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AccessToken {
    #[serde(rename = "accessToken")]
    token: String,
    #[serde(rename = "expiredAt", with = "time::serde::rfc3339")]
    expired_at: time::OffsetDateTime,
}

pub struct Client {
    client_id: String,
    client_secret: String,
    cache_dir: String,
    token: Arc<RwLock<Option<AccessToken>>>,
}

impl Client {
    pub fn new(client_id: &str, client_secret: &str, cache_dir: &str) -> Self {
        Self {
            client_id: client_id.to_owned(),
            client_secret: client_secret.to_owned(),
            cache_dir: cache_dir.to_owned(),
            token: Arc::new(RwLock::new(None)),
        }
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

    pub async fn search(&self, file_name: &str) -> RequestResult<Vec<File>> {
        self.get::<FileListResponse>(
            self.build_api_url("/api/file/list/new"),
            Some(vec![
                ("driveId", "0"),
                ("limit", "100"),
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

    pub async fn fast_upload(
        &self,
        parent_file_id: i64,
        file_name: &str,
        etag: &str,
        size: u64,
    ) -> RequestResult<Option<i64>> {
        self.post::<_, FastUploadResponse>(
            self.build_open_api_url("/upload/v2/file/create"),
            None,
            Some(&json!(
                {
                    "parentFileID": parent_file_id,
                    "filename": file_name,
                    "etag": etag,
                    "size": size,
                }
            )),
        )
        .await
        .map(|r| r.file_id)
    }

    async fn get<T: DeserializeOwned>(&self, url: String, query: Option<Vec<(&str, &str)>>) -> RequestResult<T> {
        let token = format!("Bearer {}", self.get_token().await?);
        let headers = Some(vec![
            (PLATFORM_KEY, PLATFORM_VALUE),
            (UA_KEY, UA_VALUE),
            (AUTH_KEY, token.as_str()),
        ]);

        let response: CommonResponse<T> = super::http::get(url, query, headers).await?;
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
            (UA_KEY, UA_VALUE),
            (AUTH_KEY, token.as_str()),
        ]);

        let response: CommonResponse<T> = super::http::post(url, query, headers, payload).await?;
        self.process_response(response)
    }

    fn process_response<T: DeserializeOwned>(&self, resp: CommonResponse<T>) -> RequestResult<T> {
        match resp.code {
            0 => match resp.data {
                Some(d) => Ok(d),
                None => Err(RequestError::NotFound),
            },
            401 => Err(RequestError::Unauthorized),
            429 => Err(RequestError::TooManyRequests),
            _ => Err(RequestError::Error(format!(
                "api error, code: {}, message: {}",
                resp.code, resp.message
            ))),
        }
    }

    fn build_api_url(&self, path: &str) -> String {
        format!("{}{}", API_BASE, path)
    }

    fn build_open_api_url(&self, path: &str) -> String {
        format!("{}{}", OPEN_API_BASE, path)
    }

    async fn get_token(&self) -> RequestResult<String> {
        // --- 第一次检查（无锁）---
        {
            let token_guard = self.token.read().await;
            if let Some(t) = token_guard.as_ref() {
                if !self.is_expired(t) {
                    // 缓存有效，快速返回（并发读）
                    return Ok(t.token.to_owned());
                }
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
                if let Some(t) = token {
                    if !self.is_expired(&t) {
                        // 缓存文件中的token有效，快速返回（并发读）
                        *token_guard = Some(t.clone());
                        return Ok(t.token.to_owned());
                    }
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
                Err(e) => {
                    return Err(RequestError::Error(format!(
                        "deserialize token cache file [{}] failed, {}",
                        path, e
                    )));
                }
            },
            Err(e) => {
                return Err(RequestError::Error(format!(
                    "read token cache file [{}] failed, {}",
                    path, e
                )));
            }
        }
    }

    fn write_token_to_cache_file(&self, token: &AccessToken) -> RequestResult<()> {
        if self.cache_dir.is_empty() {
            return Err(RequestError::Error("cache dir is empty".to_string()));
        }

        let path = format!("{}/{}", self.cache_dir, TOKEN_CACHE_FILE);
        if !Path::new(&self.cache_dir).exists() {
            fs::create_dir_all(&self.cache_dir)
                .map_err(|e| RequestError::Error(format!("create cache dir [{}] failed, {}", self.cache_dir, e)))?;
        }

        match serde_json::to_string(token) {
            Ok(c) => match fs::write(&path, c) {
                Ok(_) => Ok(()),
                Err(e) => {
                    return Err(RequestError::Error(format!(
                        "write token to cache file [{}] failed, {}",
                        path, e
                    )));
                }
            },
            Err(e) => {
                return Err(RequestError::Error(format!("serialize token failed, {}", e)));
            }
        }
    }

    fn is_expired(&self, token: &AccessToken) -> bool {
        token.expired_at - time::Duration::seconds(60) <= time::OffsetDateTime::now_utc()
    }

    async fn get_access_token(&self) -> RequestResult<AccessToken> {
        println!("get access token");
        let response: CommonResponse<AccessToken> = super::http::post(
            self.build_open_api_url("/api/v1/access_token"),
            None,
            Some(vec![(PLATFORM_KEY, PLATFORM_VALUE), (UA_KEY, UA_VALUE)]),
            Some(&json!({
                "clientID": self.client_id,
                "clientSecret": self.client_secret,
            })),
        )
        .await?;

        self.process_response(response)
    }
}
