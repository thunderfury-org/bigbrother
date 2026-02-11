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
struct DownloadInfo {
    #[serde(rename = "DownloadUrl")]
    download_url: String,
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
    cache_dir: String,
    token: Arc<RwLock<Option<AccessToken>>>,
}

impl Client {
    pub fn new(passport: &str, password: &str, cache_dir: &str) -> Self {
        Self {
            passport: passport.to_owned(),
            password: password.to_owned(),
            cache_dir: cache_dir.to_owned(),
            token: Arc::new(RwLock::new(None)),
        }
    }

    pub async fn get_download_url(&self, file_id: i64) -> RequestResult<String> {
        let files = self.mutli_get(&[file_id]).await?;
        match files.get(&file_id) {
            Some(f) => self
                .post::<_, DownloadInfo>(
                    self.build_api_url("/api/file/download_info"),
                    None,
                    Some(&json!(
                        {
                            "driveId": 0,
                            "FileId": file_id,
                            "Etag": f.etag,
                            "Size": f.size,
                            "S3KeyFlag": f.s3_key_flag,
                            "FileName": f.file_name,
                        }
                    )),
                )
                .await
                .map(|r| r.download_url),
            None => Err(RequestError::NotFound(format!("file {} not found", file_id))),
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
            let files = chunk.iter().map(|id| json!({"FileId": id})).collect::<Vec<_>>();
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

        let parts = folder_path.split('/').filter(|s| !s.is_empty()).collect::<Vec<_>>();
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

        let parts = path.split('/').filter(|s| !s.is_empty()).collect::<Vec<_>>();
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
        let files = file_ids.iter().map(|id| json!({"FileId": id})).collect::<Vec<_>>();
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

    async fn get<T: DeserializeOwned>(&self, url: String, query: Option<Vec<(&str, &str)>>) -> RequestResult<T> {
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
        format!("{}{}", API_BASE, path)
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
            fs::create_dir_all(&self.cache_dir)
                .map_err(|e| RequestError::Error(format!("create cache dir [{}] failed, {}", self.cache_dir, e)))?;
        }

        match serde_json::to_string(token) {
            Ok(c) => match fs::write(&path, c) {
                Ok(_) => Ok(()),
                Err(e) => Err(RequestError::Error(format!(
                    "write token to cache file [{}] failed, {}",
                    path, e
                ))),
            },
            Err(e) => Err(RequestError::Error(format!("serialize token failed, {}", e))),
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
