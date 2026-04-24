use reqwest::{Client as HttpClient, header};
use serde::Deserialize;

use super::{RequestError, RequestResult, http};

const API_URL: &str = "https://cloud.189.cn";

#[derive(Debug, Deserialize)]
pub struct ShareInfo {
    #[serde(rename = "res_code")]
    res_code: i32,
    #[serde(rename = "res_message")]
    res_message: String,

    #[serde(rename = "fileId")]
    pub file_id: String,
    #[serde(rename = "fileName")]
    pub file_name: String,
    #[serde(rename = "shareId")]
    pub share_id: i64,
    #[serde(rename = "shareMode")]
    pub share_mode: i32,
}

#[derive(Debug, Deserialize)]
pub struct File {
    #[serde(rename = "id")]
    pub id: String,
    #[serde(rename = "name")]
    pub name: String,
    #[serde(rename = "size")]
    pub size: u64,
    #[serde(rename = "md5")]
    pub md5: String,
}

#[derive(Debug, Deserialize)]
pub struct Folder {
    #[serde(rename = "id")]
    pub id: String,
    #[serde(rename = "name")]
    pub name: String,
}

#[derive(Debug, Deserialize)]
struct FileListResponse {
    #[serde(rename = "count")]
    count: u32,
    #[serde(rename = "fileList")]
    file_list: Vec<File>,
    #[serde(rename = "folderList")]
    folder_list: Vec<Folder>,
}

#[derive(Debug, Deserialize)]
struct ListShareFileResponse {
    #[serde(rename = "res_code")]
    res_code: i32,
    #[serde(rename = "res_message")]
    res_message: String,

    #[serde(rename = "fileListAO")]
    file_list: FileListResponse,
}

#[derive(Debug, Deserialize)]
struct DownloadUrlResponse {
    #[serde(rename = "res_code")]
    res_code: i32,
    #[serde(alias = "res_message", alias = "res_msg", default)]
    res_message: String,
    #[serde(rename = "fileDownloadUrl", default)]
    file_download_url: String,
}

#[derive(Debug, Clone)]
pub struct Client {
    host: String,
    cookie: String,
    http_client: HttpClient,
}

impl Client {
    pub fn new(cookie: &str) -> Self {
        Self {
            host: API_URL.to_owned(),
            cookie: cookie.trim().to_owned(),
            http_client: HttpClient::builder()
                .redirect(reqwest::redirect::Policy::limited(10))
                .timeout(std::time::Duration::from_secs(60))
                .build()
                .expect("failed to create pan189 http client"),
        }
    }

    #[cfg(test)]
    fn with_host(host: &str) -> Self {
        Self {
            host: host.to_owned(),
            cookie: String::new(),
            http_client: HttpClient::new(),
        }
    }

    pub async fn get_share_info(&self, share_code: &str) -> RequestResult<ShareInfo> {
        let info: ShareInfo = http::get(
            self.build_api_url("/api/open/share/getShareInfoByCodeV2.action"),
            Some(vec![("shareCode", share_code)]),
            Some(vec![("sign-type", "1")]),
        )
        .await?;
        if info.res_code != 0 {
            return Err(RequestError::Error(format!(
                "get share info failed, res_code: {}, res_message: {}",
                info.res_code, info.res_message
            )));
        }
        Ok(info)
    }

    /// 列出分享目录下的文件和文件夹
    ///
    /// # Returns
    ///
    /// * `RequestResult<(Vec<Folder>, Vec<File>)>` - 文件夹列表和文件列表
    pub async fn list_share_files(
        &self,
        share_id: i64,
        share_mode: i32,
        file_id: &str,
    ) -> RequestResult<(Vec<Folder>, Vec<File>)> {
        let mut folder_list = Vec::new();
        let mut file_list = Vec::new();

        let page_size = 100;
        let mut page_num = 1;
        loop {
            let response: ListShareFileResponse = http::get(
                self.build_api_url("/api/open/share/listShareDir.action"),
                Some(vec![
                    ("pageNum", page_num.to_string().as_str()),
                    ("pageSize", page_size.to_string().as_str()),
                    ("shareId", share_id.to_string().as_str()),
                    ("shareMode", share_mode.to_string().as_str()),
                    ("shareDirFileId", file_id),
                    ("orderBy", "filename"),
                    ("descending", "false"),
                    ("fileId", file_id),
                    ("isFolder", "true"),
                ]),
                Some(vec![("sign-type", "1")]),
            )
            .await?;
            if response.res_code != 0 {
                return Err(RequestError::Error(format!(
                    "list share files failed, res_code: {}, res_message: {}",
                    response.res_code, response.res_message
                )));
            }
            folder_list.extend(response.file_list.folder_list);
            file_list.extend(response.file_list.file_list);
            if response.file_list.count < page_size {
                break;
            }
            page_num += 1;
        }

        Ok((folder_list, file_list))
    }

    pub async fn download_share_file(
        &self,
        share_id: i64,
        file_id: &str,
    ) -> RequestResult<Vec<u8>> {
        if self.cookie.is_empty() {
            return Err(RequestError::Error(
                "pan189.cookie is required to download shared CAS files".into(),
            ));
        }

        let download_url = self.get_shared_file_download_url(share_id, file_id).await?;
        self.download_bytes(&download_url).await
    }

    async fn get_shared_file_download_url(
        &self,
        share_id: i64,
        file_id: &str,
    ) -> RequestResult<String> {
        let response = self
            .http_client
            .get(self.build_api_url("/api/open/file/getFileDownloadUrl.action"))
            .query(&[
                ("noCache", "0.25105336592640093".to_owned()),
                ("fileId", file_id.to_owned()),
                ("dt", "1".to_owned()),
                ("shareId", share_id.to_string()),
            ])
            .headers(self.cookie_headers())
            .send()
            .await?;
        let response: DownloadUrlResponse = process_json_response(response).await?;
        if response.res_code != 0 || response.file_download_url.is_empty() {
            return Err(RequestError::Error(format!(
                "get pan189 shared file download url failed, res_code: {}, res_message: {}",
                response.res_code, response.res_message
            )));
        }
        Ok(response.file_download_url)
    }

    async fn download_bytes(&self, url: &str) -> RequestResult<Vec<u8>> {
        let response = self
            .http_client
            .get(url)
            .header(header::USER_AGENT, http::UA_VALUE)
            .send()
            .await?;
        let status = response.status();
        let payload = response.bytes().await?;
        if status.is_success() {
            Ok(payload.to_vec())
        } else {
            Err(RequestError::Error(format!(
                "download pan189 file failed, status: {status}, payload: {payload:?}"
            )))
        }
    }

    fn cookie_headers(&self) -> header::HeaderMap {
        let mut headers = header::HeaderMap::new();
        headers.insert(
            header::USER_AGENT,
            header::HeaderValue::from_static(http::UA_VALUE),
        );
        headers.insert(
            header::ACCEPT,
            header::HeaderValue::from_static("application/json;charset=UTF-8"),
        );
        headers.insert(
            header::COOKIE,
            header::HeaderValue::from_str(&self.cookie)
                .unwrap_or_else(|_| header::HeaderValue::from_static("")),
        );
        headers.insert(
            header::HeaderName::from_static("sign-type"),
            header::HeaderValue::from_static("1"),
        );
        headers
    }

    #[inline]
    fn build_api_url(&self, path: &str) -> String {
        format!("{}{path}", self.host)
    }
}

async fn process_json_response<T: for<'de> Deserialize<'de>>(
    response: reqwest::Response,
) -> RequestResult<T> {
    let status = response.status();
    let url = response.url().to_string();
    let payload = response.text().await?;
    if !status.is_success() {
        return Err(RequestError::Error(format!(
            "http request to {url} failed, status: {status}, payload: {payload}"
        )));
    }
    serde_json::from_str(&payload).map_err(|e| {
        RequestError::Error(format!(
            "http request to {url} failed, decode payload failed, {e}, payload: {payload}"
        ))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{header, method, path, query_param},
    };

    fn client(server: &MockServer) -> Client {
        Client::with_host(server.uri().as_str())
    }

    fn cookie_client(server: &MockServer) -> Client {
        let mut client = Client::new("COOKIE_LOGIN_USER=token; JSESSIONID=session");
        client.host = server.uri();
        client
    }

    #[tokio::test]
    async fn get_share_info_returns_share_info_on_success() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/open/share/getShareInfoByCodeV2.action"))
            .and(query_param("shareCode", "abc123"))
            .and(header("sign-type", "1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "res_code": 0,
                "res_message": "success",
                "fileId": "file-1",
                "fileName": "movie.mkv",
                "shareId": 42,
                "shareMode": 1
            })))
            .mount(&server)
            .await;

        let result = client(&server).get_share_info("abc123").await.unwrap();

        assert_eq!(result.file_id, "file-1");
        assert_eq!(result.file_name, "movie.mkv");
        assert_eq!(result.share_id, 42);
        assert_eq!(result.share_mode, 1);
    }

    #[tokio::test]
    async fn get_share_info_returns_error_when_res_code_is_non_zero() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/open/share/getShareInfoByCodeV2.action"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "res_code": 500,
                "res_message": "boom",
                "fileId": "",
                "fileName": "",
                "shareId": 0,
                "shareMode": 0
            })))
            .mount(&server)
            .await;

        let error = client(&server).get_share_info("abc123").await.unwrap_err();

        match error {
            RequestError::Error(message) => {
                assert!(message.contains("get share info failed"));
                assert!(message.contains("boom"));
            }
            other => panic!("expected business error, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn list_share_files_collects_multiple_pages() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/open/share/listShareDir.action"))
            .and(query_param("pageNum", "1"))
            .and(query_param("pageSize", "100"))
            .and(query_param("shareId", "42"))
            .and(query_param("shareMode", "1"))
            .and(query_param("shareDirFileId", "root"))
            .and(query_param("fileId", "root"))
            .and(header("sign-type", "1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "res_code": 0,
                "res_message": "ok",
                "fileListAO": {
                    "count": 100,
                    "fileList": [{
                        "id": "file-1",
                        "name": "episode-01.mkv",
                        "size": 1000,
                        "md5": "md5-1"
                    }],
                    "folderList": [{
                        "id": "folder-1",
                        "name": "Season 1"
                    }]
                }
            })))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/api/open/share/listShareDir.action"))
            .and(query_param("pageNum", "2"))
            .and(query_param("pageSize", "100"))
            .and(query_param("shareId", "42"))
            .and(query_param("shareMode", "1"))
            .and(query_param("shareDirFileId", "root"))
            .and(query_param("fileId", "root"))
            .and(header("sign-type", "1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "res_code": 0,
                "res_message": "ok",
                "fileListAO": {
                    "count": 1,
                    "fileList": [{
                        "id": "file-2",
                        "name": "episode-02.mkv",
                        "size": 2000,
                        "md5": "md5-2"
                    }],
                    "folderList": [{
                        "id": "folder-2",
                        "name": "Extras"
                    }]
                }
            })))
            .mount(&server)
            .await;

        let (folders, files) = client(&server)
            .list_share_files(42, 1, "root")
            .await
            .unwrap();

        assert_eq!(folders.len(), 2);
        assert_eq!(folders[0].id, "folder-1");
        assert_eq!(folders[1].id, "folder-2");
        assert_eq!(files.len(), 2);
        assert_eq!(files[0].name, "episode-01.mkv");
        assert_eq!(files[1].name, "episode-02.mkv");
    }

    #[tokio::test]
    async fn list_share_files_returns_error_when_res_code_is_non_zero() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/open/share/listShareDir.action"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "res_code": 403,
                "res_message": "denied",
                "fileListAO": {
                    "count": 0,
                    "fileList": [],
                    "folderList": []
                }
            })))
            .mount(&server)
            .await;

        let error = client(&server)
            .list_share_files(42, 1, "root")
            .await
            .unwrap_err();

        match error {
            RequestError::Error(message) => {
                assert!(message.contains("list share files failed"));
                assert!(message.contains("denied"));
            }
            other => panic!("expected business error, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn download_share_file_uses_account_cookie_and_share_id() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/open/file/getFileDownloadUrl.action"))
            .and(query_param("fileId", "file-1"))
            .and(query_param("shareId", "42"))
            .and(query_param("dt", "1"))
            .and(header(
                "cookie",
                "COOKIE_LOGIN_USER=token; JSESSIONID=session",
            ))
            .and(header("sign-type", "1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "res_code": 0,
                "res_message": "ok",
                "fileDownloadUrl": format!("{}/download/cas", server.uri())
            })))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/download/cas"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes("cas-content"))
            .mount(&server)
            .await;

        let content = cookie_client(&server)
            .download_share_file(42, "file-1")
            .await
            .unwrap();

        assert_eq!(content, b"cas-content");
    }

    #[tokio::test]
    async fn download_share_file_requires_account_cookie() {
        let server = MockServer::start().await;

        let error = client(&server)
            .download_share_file(42, "file-1")
            .await
            .unwrap_err();

        match error {
            RequestError::Error(message) => {
                assert!(message.contains("pan189.cookie is required"));
            }
            other => panic!("expected business error, got {other:?}"),
        }
    }
}
