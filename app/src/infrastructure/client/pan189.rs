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

#[derive(Debug, Default, Clone)]
pub struct Client {
    host: String,
}

impl Client {
    pub fn new() -> Self {
        Self {
            host: API_URL.to_owned(),
        }
    }

    #[cfg(test)]
    fn with_host(host: &str) -> Self {
        Self {
            host: host.to_owned(),
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

    #[inline]
    fn build_api_url(&self, path: &str) -> String {
        format!("{}{path}", self.host)
    }
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

        let (folders, files) = client(&server).list_share_files(42, 1, "root").await.unwrap();

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
}
