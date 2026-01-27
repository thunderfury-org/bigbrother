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
pub struct Client {}

impl Client {
    pub fn new() -> Self {
        Self {}
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
        format!("{API_URL}{path}")
    }
}
