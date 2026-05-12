use serde::{Deserialize, Deserializer};

use super::{RequestError, RequestResult, http};

const API_URL: &str = "https://115cdn.com/webapi/share/snap";

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
    data: Option<ListResponseData>,
}

#[derive(Debug, Deserialize)]
struct ListResponseData {
    #[serde(rename = "count")]
    pub count: i32,

    #[serde(rename = "list")]
    pub list: Vec<FileEntry>,
}

#[derive(Debug, Default, Clone)]
pub struct Client {}

impl Client {
    pub fn new() -> Self {
        Self {}
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
            let response: ListResponse = http::get(
                API_URL,
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

            if !response.state {
                return Err(RequestError::Other(format!(
                    "list share files failed, errno: {}, error: {}",
                    response.errno, response.error
                )));
            }

            let data = response
                .data
                .ok_or_else(|| RequestError::Other("no data in response".to_string()))?;

            files.extend(data.list);

            // Check if there are more items
            if data.count < limit {
                break;
            }

            offset += limit;
        }

        Ok(files)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let data = response.data.unwrap();
        assert_eq!(data.count, 1);
        assert_eq!(data.list.len(), 1);
        assert_eq!(data.list[0].name, "新狂蟒之灾 Anaconda (2025)");
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
}
