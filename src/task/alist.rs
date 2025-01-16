use std::path::Path;

use serde::{de::DeserializeOwned, Deserialize, Serialize};

use crate::common::{
    error::{Error, Result},
    state::AppState,
};

pub struct Client<'a> {
    client: reqwest::Client,
    host: &'a str,
    api_token: &'a str,
}

impl Client<'_> {
    async fn post<I: Serialize, T: DeserializeOwned + Default>(&self, url: &str, json: &I) -> Result<T> {
        let result = self
            .client
            .post(format!("{}{}", self.host, url))
            .header("Authorization", self.api_token)
            .json(json)
            .send()
            .await;
        if result.is_err() {
            return Err(Error::Internal(format!(
                "http post {} failed, {}",
                url,
                result.err().unwrap()
            )));
        }

        let response = result.unwrap();

        if !response.status().is_success() {
            let u = response.url().to_string();
            let status = response.status();
            let body = response
                .text()
                .await
                .unwrap_or_else(|e| format!("parse response body to string error, {}", e));

            return Err(Error::Internal(format!(
                "http post {u} failed, status: {status}, body: {body}"
            )));
        }

        let text = response.text().await.unwrap();

        let json_result = serde_json::from_str::<ResponseModel<T>>(text.as_str());
        if json_result.is_err() {
            return Err(Error::Internal(format!(
                "http post {} failed, decode body failed, {}, body: {}",
                url,
                json_result.err().unwrap(),
                text
            )));
        }

        let r = json_result.unwrap();
        if r.code != 200 {
            if r.message.contains("not found") {
                return Err(Error::NotFound(r.message));
            } else {
                return Err(Error::Internal(format!(
                    "http post failed, code: {}, message: {}",
                    r.code, r.message
                )));
            }
        }

        match r.data {
            None => Ok(T::default()),
            Some(d) => Ok(d),
        }
    }

    pub async fn mkdir(&self, path: &str) -> Result<()> {
        self.post("/api/fs/mkdir", &serde_json::json!({"path": path})).await
    }

    pub async fn rename(&self, file_path: &str, new_name: &str) -> Result<()> {
        self.post(
            "/api/fs/rename",
            &serde_json::json!({
                "path": file_path,
                "name": new_name,
            }),
        )
        .await
    }

    pub async fn move_file(&self, src_dir: &str, dest_dir: &str, name: &str) -> Result<()> {
        self.post(
            "/api/fs/move",
            &serde_json::json!({
                "src_dir": src_dir,
                "dst_dir": dest_dir,
                "names": [
                    name,
                ],
            }),
        )
        .await
    }

    pub async fn list(&self, path: &str) -> Result<Vec<File>> {
        let response: ListResponse = self
            .post(
                "/api/fs/list",
                &ListRequest {
                    path,
                    page: 1,
                    per_page: 0,
                    refresh: true,
                    password: "",
                },
            )
            .await?;

        if response.content.is_none() {
            return Ok(vec![]);
        }

        let mut files = response.content.unwrap();
        for f in &mut files {
            f.path = Path::new(path).join(f.name.as_str()).to_str().unwrap().to_string();
        }

        Ok(files)
    }
}

impl<'a> TryFrom<&'a AppState> for Client<'a> {
    type Error = Error;

    fn try_from(state: &'a AppState) -> Result<Self> {
        let host = state.config.get_app_config().alist_host.as_str();
        let api_token = state.config.get_app_config().alist_api_token.as_str();

        if host.is_empty() {
            return Err(Error::Internal("alist host is empty".to_string()));
        }
        if api_token.is_empty() {
            return Err(Error::Internal("alist token not set".to_string()));
        }

        Ok(Client {
            client: state.http_client.clone(),
            host,
            api_token,
        })
    }
}

#[derive(Debug, Deserialize)]
struct ResponseModel<T> {
    pub code: i32,
    pub message: String,
    pub data: Option<T>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct File {
    pub name: String,
    #[serde(skip)]
    pub path: String,
    // pub size: u64,
    pub is_dir: bool,
}

#[derive(Debug, Serialize)]
struct ListRequest<'a> {
    pub path: &'a str,
    pub refresh: bool,
    pub page: u32,
    pub per_page: u32,
    pub password: &'a str,
}

#[derive(Debug, Deserialize, Default)]
struct ListResponse {
    pub content: Option<Vec<File>>,
}
