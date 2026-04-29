use std::{fs, path::Path, sync::Arc};

use base64::{Engine, engine::general_purpose::STANDARD};
use reqwest::{Client as HttpClient, header};
use rsa::{Pkcs1v15Encrypt, RsaPublicKey, pkcs8::DecodePublicKey, rand_core::OsRng};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

use super::{RequestError, RequestResult, http};

const WEB_API_URL: &str = "https://cloud.189.cn";
const PC_API_URL: &str = "https://api.cloud.189.cn";
const AUTH_API_URL: &str = "https://open.e.189.cn";
const PC_APP_ID: &str = "8025431004";
const PC_ACCOUNT_TYPE: &str = "02";
const PC_CLIENT_TYPE: &str = "10020";
const PC_RETURN_URL: &str = "https://m.cloud.189.cn/zhuanti/2020/loginErrorPc/index.html";
const SESSION_CACHE_FILE: &str = "session.json";
const WEB_COOKIE_CACHE_TTL_SECONDS: i64 = 30 * 24 * 60 * 60;

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

#[derive(Debug, Deserialize)]
struct ErrorCodeResponse {
    #[serde(default, alias = "errorCode", alias = "res_code")]
    error_code: String,
    #[serde(default, alias = "errorMsg", alias = "res_message", alias = "res_msg")]
    error_message: String,
}

#[derive(Debug, Deserialize)]
struct PcLoginResponse {
    #[serde(default)]
    result: i32,
    #[serde(default, rename = "toUrl")]
    to_url: String,
    #[serde(default)]
    msg: String,
}

#[derive(Debug, Deserialize)]
struct PcSessionResponse {
    #[serde(default, rename = "res_code")]
    res_code: i32,
    #[serde(default, rename = "res_message")]
    res_message: String,
    #[serde(default, rename = "sessionKey")]
    session_key: String,
    #[serde(default, rename = "sessionSecret")]
    session_secret: String,
    #[serde(default, rename = "keepAlive")]
    keep_alive: i64,
}

#[derive(Debug)]
struct PcLoginParams {
    captcha_token: String,
    lt: String,
    return_url: String,
    param_id: String,
    req_id: String,
    rsa_key: String,
}

#[derive(Debug, Clone)]
pub struct AuthConfig {
    pub username: String,
    pub password: String,
    pub cache_dir: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CachedPcSession {
    session_key: String,
    session_secret: String,
    #[serde(default)]
    web_cookie: String,
    #[serde(default, with = "time::serde::rfc3339::option")]
    web_cookie_expired_at: Option<time::OffsetDateTime>,
    #[serde(with = "time::serde::rfc3339")]
    expired_at: time::OffsetDateTime,
}

#[derive(Debug, Clone)]
pub struct Client {
    host: String,
    pc_host: String,
    auth_host: String,
    username: String,
    password: String,
    cache_dir: String,
    session: Arc<RwLock<Option<CachedPcSession>>>,
    http_client: HttpClient,
}

impl Client {
    pub fn new(config: AuthConfig) -> Self {
        Self {
            host: WEB_API_URL.to_owned(),
            pc_host: PC_API_URL.to_owned(),
            auth_host: AUTH_API_URL.to_owned(),
            username: config.username.trim().to_owned(),
            password: config.password.trim().to_owned(),
            cache_dir: config.cache_dir,
            session: Arc::new(RwLock::new(None)),
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
            pc_host: host.to_owned(),
            auth_host: host.to_owned(),
            username: String::new(),
            password: String::new(),
            cache_dir: String::new(),
            session: Arc::new(RwLock::new(None)),
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

    pub async fn download_share_file(&self, share_id: i64, file: &File) -> RequestResult<Vec<u8>> {
        let session = self.get_pc_session_with_web_cookie().await?;
        let download_url = match self
            .get_shared_file_download_url(&session.web_cookie, share_id, &file.id)
            .await
        {
            Ok(download_url) => download_url,
            Err(RequestError::Unauthorized) => {
                self.clear_cached_session().await?;
                let session = self.get_pc_session_with_web_cookie().await?;
                self.get_shared_file_download_url(&session.web_cookie, share_id, &file.id)
                    .await?
            }
            Err(err) => return Err(err),
        };
        self.download_bytes(&download_url).await
    }

    async fn get_pc_session_with_web_cookie(&self) -> RequestResult<CachedPcSession> {
        if let Some(session) = self.get_cached_session_with_valid_web_cookie().await? {
            return Ok(session);
        }

        let mut session = self.get_pc_session().await?;
        session.web_cookie = self.get_web_cookie(&session).await?;
        session.web_cookie_expired_at = Some(
            time::OffsetDateTime::now_utc() + time::Duration::seconds(WEB_COOKIE_CACHE_TTL_SECONDS),
        );
        self.write_session_to_cache_file(&session)?;
        let mut guard = self.session.write().await;
        *guard = Some(session.clone());
        Ok(session)
    }

    async fn get_cached_session_with_valid_web_cookie(
        &self,
    ) -> RequestResult<Option<CachedPcSession>> {
        {
            let guard = self.session.read().await;
            if let Some(session) = guard.as_ref()
                && self.has_valid_web_cookie(session)
            {
                return Ok(Some(session.clone()));
            }
        }

        if let Some(session) = self.read_session_from_cache_file()?
            && self.has_valid_web_cookie(&session)
        {
            let mut guard = self.session.write().await;
            *guard = Some(session.clone());
            return Ok(Some(session));
        }

        Ok(None)
    }

    async fn get_pc_session(&self) -> RequestResult<CachedPcSession> {
        {
            let guard = self.session.read().await;
            if let Some(session) = guard.as_ref()
                && !self.is_expired(session)
            {
                return Ok(session.clone());
            }
        }

        let mut guard = self.session.write().await;
        if let Some(session) = guard.as_ref()
            && !self.is_expired(session)
        {
            return Ok(session.clone());
        }

        if let Some(session) = self.read_session_from_cache_file()?
            && !self.is_expired(&session)
        {
            *guard = Some(session.clone());
            return Ok(session);
        }

        let session = self.login_pc().await?;
        self.write_session_to_cache_file(&session)?;
        *guard = Some(session.clone());
        Ok(session)
    }

    async fn login_pc(&self) -> RequestResult<CachedPcSession> {
        if self.username.is_empty() || self.password.is_empty() {
            return Err(RequestError::Error(
                "pan189.username and pan189.password are required to download shared CAS files"
                    .into(),
            ));
        }

        let params = self.get_pc_login_params().await?;
        let rsa_username = rsa_encrypt_hex(&params.rsa_key, &self.username)?;
        let rsa_password = rsa_encrypt_hex(&params.rsa_key, &self.password)?;

        let response = self
            .http_client
            .post(self.build_auth_api_url("/api/logbox/oauth2/loginSubmit.do"))
            .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
            .header(
                header::REFERER,
                self.build_auth_api_url("/api/logbox/oauth2/unifyAccountLogin.do"),
            )
            .header("Cookie", format!("LT={}", params.lt))
            .header("X-Requested-With", "XMLHttpRequest")
            .header("REQID", &params.req_id)
            .header("lt", &params.lt)
            .form(&[
                ("appKey", PC_APP_ID),
                ("accountType", PC_ACCOUNT_TYPE),
                ("userName", &rsa_username),
                ("password", &rsa_password),
                ("validateCode", ""),
                ("captchaToken", &params.captcha_token),
                ("returnUrl", &params.return_url),
                ("mailSuffix", "@189.cn"),
                ("dynamicCheck", "FALSE"),
                ("clientType", PC_CLIENT_TYPE),
                ("cb_SaveName", "1"),
                ("isOauth2", "false"),
                ("state", ""),
                ("paramId", &params.param_id),
            ])
            .send()
            .await?;
        let login: PcLoginResponse = process_json_response(response).await?;
        if login.result != 0 || login.to_url.is_empty() {
            if login.result == -133 || login.msg.contains("设备ID不存在") {
                return Err(RequestError::Error(format!(
                    "pan189 pc login requires secondary device verification, result: {}, msg: {}; 请先在天翼云盘 App 或天翼账号安全设置中完成设备校验/关闭设备锁后重试",
                    login.result, login.msg
                )));
            }
            return Err(RequestError::Error(format!(
                "pan189 pc login failed, result: {}, msg: {}",
                login.result, login.msg
            )));
        }

        let response = self
            .http_client
            .get(self.build_pc_api_url("/getSessionForPC.action"))
            .query(&[
                ("clientType", "TELEMAC"),
                ("version", "1.0.0"),
                ("channelId", "web_cloud.189.cn"),
                ("redirectURL", login.to_url.as_str()),
            ])
            .header(header::ACCEPT, "application/json;charset=UTF-8")
            .send()
            .await?;
        let session: PcSessionResponse = process_json_response(response).await?;
        if session.res_code != 0
            || session.session_key.is_empty()
            || session.session_secret.is_empty()
        {
            return Err(RequestError::Error(format!(
                "get pan189 pc session failed, res_code: {}, res_message: {}",
                session.res_code, session.res_message
            )));
        }
        let keep_alive = if session.keep_alive > 0 {
            session.keep_alive
        } else {
            3600
        };
        Ok(CachedPcSession {
            session_key: session.session_key,
            session_secret: session.session_secret,
            web_cookie: String::new(),
            web_cookie_expired_at: None,
            expired_at: time::OffsetDateTime::now_utc() + time::Duration::seconds(keep_alive),
        })
    }

    async fn get_web_cookie(&self, session: &CachedPcSession) -> RequestResult<String> {
        let client = HttpClient::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .map_err(|e| RequestError::Error(format!("create pan189 cookie client failed: {e}")))?;
        let response = client
            .get(self.build_api_url("/api/portal/ssoLogin.action"))
            .query(&[
                ("sessionKey", session.session_key.as_str()),
                (
                    "redirectUrl",
                    "https://cloud.189.cn/main.action#share/sendout",
                ),
            ])
            .header(header::USER_AGENT, http::UA_VALUE)
            .send()
            .await?;
        if !response.status().is_redirection() && !response.status().is_success() {
            let status = response.status();
            let url = response.url().to_string();
            let payload = response.text().await?;
            return Err(RequestError::Error(format!(
                "http request to {url} failed, status: {status}, payload: {payload}"
            )));
        }
        let cookies = response
            .headers()
            .get_all(header::SET_COOKIE)
            .iter()
            .filter_map(|value| value.to_str().ok())
            .filter_map(|value| value.split(';').next())
            .map(str::trim)
            .filter(|cookie| !cookie.is_empty())
            .map(ToOwned::to_owned)
            .collect::<Vec<_>>();
        if !cookies
            .iter()
            .any(|cookie| cookie.starts_with("COOKIE_LOGIN_USER="))
        {
            return Err(RequestError::Error(
                "get pan189 web cookie from pc session failed, COOKIE_LOGIN_USER missing".into(),
            ));
        }
        Ok(cookies.join("; "))
    }

    async fn get_pc_login_params(&self) -> RequestResult<PcLoginParams> {
        let response = self
            .http_client
            .get(self.build_api_url("/api/portal/unifyLoginForPC.action"))
            .query(&[
                ("appId", PC_APP_ID),
                ("clientType", PC_CLIENT_TYPE),
                ("returnURL", PC_RETURN_URL),
                (
                    "timeStamp",
                    &chrono::Utc::now().timestamp_millis().to_string(),
                ),
            ])
            .send()
            .await?;
        let status = response.status();
        let url = response.url().to_string();
        let payload = response.text().await?;
        if !status.is_success() {
            return Err(RequestError::Error(format!(
                "http request to {url} failed, status: {status}, payload: {payload}"
            )));
        }
        Ok(PcLoginParams {
            captcha_token: capture_login_param(&payload, "captchaToken' value='(.+?)'")?,
            lt: capture_login_param(&payload, "lt = \"(.+?)\"")?,
            return_url: capture_login_param(&payload, "returnUrl = '(.+?)'")?,
            param_id: capture_login_param(&payload, "paramId = \"(.+?)\"")?,
            req_id: capture_login_param(&payload, "reqId = \"(.+?)\"")?,
            rsa_key: capture_login_param(&payload, "j_rsaKey\" value=\"(.+?)\"")?,
        })
    }

    async fn get_shared_file_download_url(
        &self,
        web_cookie: &str,
        share_id: i64,
        file_id: &str,
    ) -> RequestResult<String> {
        let no_cache = format!(
            "0.{}",
            chrono::Utc::now()
                .timestamp_nanos_opt()
                .unwrap_or_default()
                .abs()
        );
        let share_id = share_id.to_string();
        let response = self
            .http_client
            .get(self.build_api_url("/api/open/file/getFileDownloadUrl.action"))
            .query(&[
                ("noCache", no_cache.as_str()),
                ("fileId", file_id),
                ("dt", "1"),
                ("shareId", share_id.as_str()),
            ])
            .header(header::ACCEPT, "application/json;charset=UTF-8")
            .header(header::COOKIE, web_cookie)
            .header("sign-type", "1")
            .send()
            .await?;
        process_download_url_response(response, "get pan189 shared file download url").await
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

    #[inline]
    fn build_api_url(&self, path: &str) -> String {
        format!("{}{path}", self.host)
    }

    #[inline]
    fn build_pc_api_url(&self, path: &str) -> String {
        format!("{}{path}", self.pc_host)
    }

    #[inline]
    fn build_auth_api_url(&self, path: &str) -> String {
        format!("{}{path}", self.auth_host)
    }

    fn read_session_from_cache_file(&self) -> RequestResult<Option<CachedPcSession>> {
        if self.cache_dir.is_empty() {
            return Ok(None);
        }

        let path = format!("{}/{}", self.cache_dir, SESSION_CACHE_FILE);
        if !Path::new(&path).exists() {
            return Ok(None);
        }

        let content = fs::read_to_string(&path).map_err(|e| {
            RequestError::Error(format!(
                "read pan189 session cache file [{path}] failed, {e}"
            ))
        })?;

        match serde_json::from_str(&content) {
            Ok(session) => Ok(Some(session)),
            Err(_) => {
                let _ = fs::remove_file(&path);
                Ok(None)
            }
        }
    }

    fn write_session_to_cache_file(&self, session: &CachedPcSession) -> RequestResult<()> {
        if self.cache_dir.is_empty() {
            return Ok(());
        }

        let path = format!("{}/{}", self.cache_dir, SESSION_CACHE_FILE);
        if !Path::new(&self.cache_dir).exists() {
            fs::create_dir_all(&self.cache_dir).map_err(|e| {
                RequestError::Error(format!(
                    "create pan189 cache dir [{}] failed, {}",
                    self.cache_dir, e
                ))
            })?;
        }

        serde_json::to_string(session)
            .map_err(|e| RequestError::Error(format!("serialize pan189 session failed, {e}")))
            .and_then(|content| {
                fs::write(&path, content).map_err(|e| {
                    RequestError::Error(format!(
                        "write pan189 session cache file [{path}] failed, {e}"
                    ))
                })
            })
    }

    async fn clear_cached_session(&self) -> RequestResult<()> {
        let mut guard = self.session.write().await;
        *guard = None;
        if self.cache_dir.is_empty() {
            return Ok(());
        }
        let path = format!("{}/{}", self.cache_dir, SESSION_CACHE_FILE);
        if Path::new(&path).exists() {
            fs::remove_file(&path).map_err(|e| {
                RequestError::Error(format!(
                    "remove pan189 session cache file [{path}] failed, {e}"
                ))
            })?;
        }
        Ok(())
    }

    fn is_expired(&self, session: &CachedPcSession) -> bool {
        session.expired_at - time::Duration::seconds(60) <= time::OffsetDateTime::now_utc()
    }

    fn has_valid_web_cookie(&self, session: &CachedPcSession) -> bool {
        if session.web_cookie.is_empty() {
            return false;
        }
        session.web_cookie_expired_at.is_some_and(|expired_at| {
            expired_at - time::Duration::seconds(60) > time::OffsetDateTime::now_utc()
        })
    }
}

fn capture_login_param(payload: &str, pattern: &str) -> RequestResult<String> {
    let captures = regex::Regex::new(pattern)
        .map_err(|e| RequestError::Error(format!("invalid pan189 login regex: {e}")))?
        .captures(payload)
        .ok_or_else(|| {
            RequestError::Error(format!(
                "parse pan189 login page failed, pattern not found: {pattern}"
            ))
        })?;
    captures
        .get(1)
        .map(|value| value.as_str().to_owned())
        .ok_or_else(|| {
            RequestError::Error(format!(
                "parse pan189 login page failed, capture group missing: {pattern}"
            ))
        })
}

fn rsa_encrypt_hex(public_key: &str, value: &str) -> RequestResult<String> {
    let public_key = if public_key.contains("BEGIN PUBLIC KEY") {
        RsaPublicKey::from_public_key_pem(public_key)
            .map_err(|e| RequestError::Error(format!("parse pan189 rsa public key failed: {e}")))?
    } else {
        let der = STANDARD.decode(public_key).map_err(|e| {
            RequestError::Error(format!("decode pan189 rsa public key failed: {e}"))
        })?;
        RsaPublicKey::from_public_key_der(&der)
            .map_err(|e| RequestError::Error(format!("parse pan189 rsa public key failed: {e}")))?
    };
    let mut rng = OsRng;
    let encrypted = public_key
        .encrypt(&mut rng, Pkcs1v15Encrypt, value.as_bytes())
        .map_err(|e| RequestError::Error(format!("encrypt pan189 login field failed: {e}")))?;
    Ok(format!("{{RSA}}{}", hex::encode_upper(encrypted)))
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

async fn process_download_url_response(
    response: reqwest::Response,
    context: &str,
) -> RequestResult<String> {
    let status = response.status();
    let url = response.url().to_string();
    let payload = response.text().await?;
    if !status.is_success() {
        if is_pan189_auth_error_payload(&payload) {
            return Err(RequestError::Unauthorized);
        }
        return Err(RequestError::Error(format!(
            "http request to {url} failed, status: {status}, payload: {payload}"
        )));
    }

    if is_pan189_auth_error_payload(&payload) {
        return Err(RequestError::Unauthorized);
    }

    if let Ok(response) = serde_json::from_str::<DownloadUrlResponse>(&payload) {
        if response.res_code == 0 && !response.file_download_url.is_empty() {
            return Ok(response.file_download_url);
        }
        return Err(RequestError::Error(format!(
            "{context} failed, res_code: {}, res_message: {}",
            response.res_code, response.res_message
        )));
    }

    let Some(start) = payload.find("<fileDownloadUrl>") else {
        return Err(RequestError::Error(format!(
            "{context} failed, payload does not contain fileDownloadUrl: {payload}"
        )));
    };
    let value_start = start + "<fileDownloadUrl>".len();
    let Some(end) = payload[value_start..].find("</fileDownloadUrl>") else {
        return Err(RequestError::Error(format!(
            "{context} failed, malformed fileDownloadUrl payload: {payload}"
        )));
    };
    let url = payload[value_start..value_start + end]
        .replace("&amp;", "&")
        .trim()
        .to_owned();
    if url.is_empty() {
        return Err(RequestError::Error(format!(
            "{context} failed, fileDownloadUrl is empty"
        )));
    }
    Ok(url)
}

fn is_pan189_auth_error_payload(payload: &str) -> bool {
    if let Ok(response) = serde_json::from_str::<ErrorCodeResponse>(payload) {
        return is_pan189_auth_error(&response.error_code, &response.error_message);
    }

    is_pan189_auth_error("", payload)
}

fn is_pan189_auth_error(code: &str, message: &str) -> bool {
    let code = code.to_ascii_lowercase();
    let message = message.to_ascii_lowercase();
    code.contains("invalidsessionkey")
        || code.contains("unauthorized")
        || message.contains("invalidsessionkey")
        || message.contains("cookieusersession is null")
        || message.contains("cookieusersession=null")
        || message.contains("cookieusersession is invalid")
        || message.contains("sessionkey")
            && (message.contains("invalid") || message.contains("expired"))
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

    fn unique_cache_dir() -> String {
        let nanos = chrono::Utc::now()
            .timestamp_nanos_opt()
            .unwrap_or_default()
            .abs();
        format!("/tmp/pan189-tests-{nanos}")
    }

    async fn set_session_for_test(client: &Client, session_key: &str, session_secret: &str) {
        let mut guard = client.session.write().await;
        *guard = Some(CachedPcSession {
            session_key: session_key.to_owned(),
            session_secret: session_secret.to_owned(),
            web_cookie: "COOKIE_LOGIN_USER=test-user".to_owned(),
            web_cookie_expired_at: Some(time::OffsetDateTime::now_utc() + time::Duration::days(30)),
            expired_at: time::OffsetDateTime::now_utc() + time::Duration::hours(1),
        });
    }

    fn account_client(server: &MockServer) -> Client {
        let mut client = Client::new(AuthConfig {
            username: "user".into(),
            password: "password".into(),
            cache_dir: unique_cache_dir(),
        });
        client.host = server.uri();
        client.pc_host = server.uri();
        client.auth_host = server.uri();
        client
    }

    fn account_client_with_cache_dir(server: &MockServer, cache_dir: &str) -> Client {
        let mut client = Client::new(AuthConfig {
            username: "user".into(),
            password: "password".into(),
            cache_dir: cache_dir.to_owned(),
        });
        client.host = server.uri();
        client.pc_host = server.uri();
        client.auth_host = server.uri();
        client
    }

    fn unauthenticated_client(server: &MockServer) -> Client {
        let mut client = Client::new(AuthConfig {
            username: String::new(),
            password: String::new(),
            cache_dir: String::new(),
        });
        client.host = server.uri();
        client.pc_host = server.uri();
        client.auth_host = server.uri();
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
    async fn download_share_file_uses_cached_pc_session() {
        let server = MockServer::start().await;
        let client = account_client(&server);
        set_session_for_test(&client, "session-key", "session-secret").await;
        Mock::given(method("GET"))
            .and(path("/api/open/file/getFileDownloadUrl.action"))
            .and(query_param("fileId", "file-1"))
            .and(query_param("shareId", "42"))
            .and(query_param("dt", "1"))
            .and(header("cookie", "COOKIE_LOGIN_USER=test-user"))
            .and(header("sign-type", "1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "res_code": 0,
                "res_message": "ok",
                "fileDownloadUrl": format!("{}/download/share-cas", server.uri())
            })))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/download/share-cas"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes("share-cas-content"))
            .mount(&server)
            .await;

        let content = client
            .download_share_file(
                42,
                &File {
                    id: "file-1".into(),
                    name: "cas.json".into(),
                    size: 10,
                    md5: "md5".into(),
                },
            )
            .await
            .unwrap();

        assert_eq!(content, b"share-cas-content");
    }

    #[tokio::test]
    async fn download_share_file_logs_in_when_pc_session_is_missing() {
        let server = MockServer::start().await;
        let public_key = "MFwwDQYJKoZIhvcNAQEBBQADSwAwSAJBAM/ZmsCHPXwsT5IIojNjfRD5HgXCi/+1yHeAbQBZBFtZYPhQxT1cDyHUd3bL3jd5h7n51VqzapGRMGl6MPUN2PUCAwEAAQ==";
        Mock::given(method("GET"))
            .and(path("/api/portal/unifyLoginForPC.action"))
            .and(query_param("appId", PC_APP_ID))
            .and(query_param("clientType", PC_CLIENT_TYPE))
            .respond_with(ResponseTemplate::new(200).set_body_string(format!(
                r#"
                <input name='captchaToken' value='captcha-token'/>
                <script>
                  lt = "lt-token";
                  returnUrl = '{}';
                  paramId = "param-id";
                  reqId = "req-id";
                </script>
                <input id="j_rsaKey" value="{public_key}"/>
                "#,
                PC_RETURN_URL
            )))
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/api/logbox/oauth2/loginSubmit.do"))
            .and(header("REQID", "req-id"))
            .and(header("lt", "lt-token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "result": 0,
                "toUrl": "https://redirect.example/session"
            })))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/getSessionForPC.action"))
            .and(query_param("clientType", "TELEMAC"))
            .and(query_param("channelId", "web_cloud.189.cn"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "res_code": 0,
                "res_message": "ok",
                "sessionKey": "session-key",
                "sessionSecret": "session-secret",
                "keepAlive": 3600
            })))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/api/portal/ssoLogin.action"))
            .and(query_param("sessionKey", "session-key"))
            .respond_with(
                ResponseTemplate::new(302)
                    .insert_header("set-cookie", "COOKIE_LOGIN_USER=login-user; Path=/"),
            )
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/api/open/file/getFileDownloadUrl.action"))
            .and(query_param("fileId", "file-1"))
            .and(query_param("shareId", "42"))
            .and(query_param("dt", "1"))
            .and(header("cookie", "COOKIE_LOGIN_USER=login-user"))
            .and(header("sign-type", "1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "res_code": 0,
                "res_message": "ok",
                "fileDownloadUrl": format!("{}/download/login-cas", server.uri())
            })))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/download/login-cas"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes("login-cas-content"))
            .mount(&server)
            .await;

        let content = account_client(&server)
            .download_share_file(
                42,
                &File {
                    id: "file-1".into(),
                    name: "cas.json".into(),
                    size: 10,
                    md5: "md5".into(),
                },
            )
            .await
            .unwrap();

        assert_eq!(content, b"login-cas-content");
    }

    #[tokio::test]
    async fn download_share_file_reads_session_from_cache_file() {
        let server = MockServer::start().await;
        let cache_dir = unique_cache_dir();
        let cached_session = CachedPcSession {
            session_key: "cached-session-key".into(),
            session_secret: "cached-session-secret".into(),
            web_cookie: "COOKIE_LOGIN_USER=cached-user".into(),
            web_cookie_expired_at: Some(time::OffsetDateTime::now_utc() + time::Duration::days(30)),
            expired_at: time::OffsetDateTime::now_utc() + time::Duration::hours(1),
        };
        let writer = account_client_with_cache_dir(&server, &cache_dir);
        writer.write_session_to_cache_file(&cached_session).unwrap();
        let client = account_client_with_cache_dir(&server, &cache_dir);

        Mock::given(method("GET"))
            .and(path("/api/open/file/getFileDownloadUrl.action"))
            .and(query_param("fileId", "file-1"))
            .and(query_param("shareId", "42"))
            .and(query_param("dt", "1"))
            .and(header("cookie", "COOKIE_LOGIN_USER=cached-user"))
            .and(header("sign-type", "1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "res_code": 0,
                "res_message": "ok",
                "fileDownloadUrl": format!("{}/download/cached-cas", server.uri())
            })))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/download/cached-cas"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes("cached-cas-content"))
            .mount(&server)
            .await;

        let content = client
            .download_share_file(
                42,
                &File {
                    id: "file-1".into(),
                    name: "cas.json".into(),
                    size: 10,
                    md5: "md5".into(),
                },
            )
            .await
            .unwrap();

        assert_eq!(content, b"cached-cas-content");
        let _ = fs::remove_dir_all(cache_dir);
    }

    #[tokio::test]
    async fn read_session_from_cache_file_ignores_corrupt_cache() {
        let server = MockServer::start().await;
        let cache_dir = unique_cache_dir();
        fs::create_dir_all(&cache_dir).unwrap();
        fs::write(format!("{cache_dir}/{SESSION_CACHE_FILE}"), "{not-json").unwrap();
        let client = account_client_with_cache_dir(&server, &cache_dir);

        let loaded = client.read_session_from_cache_file().unwrap();

        assert!(loaded.is_none());
        assert!(!Path::new(&format!("{cache_dir}/{SESSION_CACHE_FILE}")).exists());
        let _ = fs::remove_dir_all(cache_dir);
    }

    #[tokio::test]
    async fn download_share_file_uses_cached_web_cookie_when_pc_session_expired() {
        let server = MockServer::start().await;
        let cache_dir = unique_cache_dir();
        let cached_session = CachedPcSession {
            session_key: "expired-session-key".into(),
            session_secret: "expired-session-secret".into(),
            web_cookie: "COOKIE_LOGIN_USER=still-valid-web-user".into(),
            web_cookie_expired_at: Some(time::OffsetDateTime::now_utc() + time::Duration::days(30)),
            expired_at: time::OffsetDateTime::now_utc() - time::Duration::minutes(10),
        };
        let writer = account_client_with_cache_dir(&server, &cache_dir);
        writer.write_session_to_cache_file(&cached_session).unwrap();
        let client = account_client_with_cache_dir(&server, &cache_dir);

        Mock::given(method("GET"))
            .and(path("/api/open/file/getFileDownloadUrl.action"))
            .and(query_param("fileId", "file-1"))
            .and(query_param("shareId", "42"))
            .and(header("cookie", "COOKIE_LOGIN_USER=still-valid-web-user"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "res_code": 0,
                "res_message": "ok",
                "fileDownloadUrl": format!("{}/download/web-cookie-cas", server.uri())
            })))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/download/web-cookie-cas"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes("web-cookie-cas-content"))
            .mount(&server)
            .await;

        let content = client
            .download_share_file(
                42,
                &File {
                    id: "file-1".into(),
                    name: "cas.json".into(),
                    size: 10,
                    md5: "md5".into(),
                },
            )
            .await
            .unwrap();

        assert_eq!(content, b"web-cookie-cas-content");
        let _ = fs::remove_dir_all(cache_dir);
    }

    #[tokio::test]
    async fn download_share_file_relogs_in_only_when_web_cookie_is_invalid() {
        let server = MockServer::start().await;
        let cache_dir = unique_cache_dir();
        let cached_session = CachedPcSession {
            session_key: "cached-session-key".into(),
            session_secret: "cached-session-secret".into(),
            web_cookie: "COOKIE_LOGIN_USER=expired-web-user".into(),
            web_cookie_expired_at: Some(time::OffsetDateTime::now_utc() + time::Duration::days(30)),
            expired_at: time::OffsetDateTime::now_utc() + time::Duration::hours(1),
        };
        let writer = account_client_with_cache_dir(&server, &cache_dir);
        writer.write_session_to_cache_file(&cached_session).unwrap();
        let public_key = "MFwwDQYJKoZIhvcNAQEBBQADSwAwSAJBAM/ZmsCHPXwsT5IIojNjfRD5HgXCi/+1yHeAbQBZBFtZYPhQxT1cDyHUd3bL3jd5h7n51VqzapGRMGl6MPUN2PUCAwEAAQ==";

        Mock::given(method("GET"))
            .and(path("/api/open/file/getFileDownloadUrl.action"))
            .and(header("cookie", "COOKIE_LOGIN_USER=expired-web-user"))
            .respond_with(ResponseTemplate::new(400).set_body_json(serde_json::json!({
                "errorCode": "InvalidSessionKey",
                "errorMsg": "cookieUserSession is null or invalid"
            })))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/api/portal/unifyLoginForPC.action"))
            .respond_with(ResponseTemplate::new(200).set_body_string(format!(
                r#"
                <input name='captchaToken' value='captcha-token'/>
                <script>
                  lt = "lt-token";
                  returnUrl = '{}';
                  paramId = "param-id";
                  reqId = "req-id";
                </script>
                <input id="j_rsaKey" value="{public_key}"/>
                "#,
                PC_RETURN_URL
            )))
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/api/logbox/oauth2/loginSubmit.do"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "result": 0,
                "toUrl": "https://redirect.example/session"
            })))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/getSessionForPC.action"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "res_code": 0,
                "res_message": "ok",
                "sessionKey": "new-session-key",
                "sessionSecret": "new-session-secret",
                "keepAlive": 3600
            })))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/api/portal/ssoLogin.action"))
            .and(query_param("sessionKey", "new-session-key"))
            .respond_with(
                ResponseTemplate::new(302)
                    .insert_header("set-cookie", "COOKIE_LOGIN_USER=new-web-user; Path=/"),
            )
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/api/open/file/getFileDownloadUrl.action"))
            .and(header("cookie", "COOKIE_LOGIN_USER=new-web-user"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "res_code": 0,
                "res_message": "ok",
                "fileDownloadUrl": format!("{}/download/relogin-cas", server.uri())
            })))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/download/relogin-cas"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes("relogin-cas-content"))
            .mount(&server)
            .await;

        let content = account_client_with_cache_dir(&server, &cache_dir)
            .download_share_file(
                42,
                &File {
                    id: "file-1".into(),
                    name: "cas.json".into(),
                    size: 10,
                    md5: "md5".into(),
                },
            )
            .await
            .unwrap();

        assert_eq!(content, b"relogin-cas-content");
        let _ = fs::remove_dir_all(cache_dir);
    }

    #[tokio::test]
    async fn download_share_file_does_not_relogin_on_permission_denied() {
        let server = MockServer::start().await;
        let client = account_client(&server);
        set_session_for_test(&client, "session-key", "session-secret").await;
        Mock::given(method("GET"))
            .and(path("/api/open/file/getFileDownloadUrl.action"))
            .and(header("cookie", "COOKIE_LOGIN_USER=test-user"))
            .respond_with(ResponseTemplate::new(400).set_body_string(
                r#"<?xml version="1.0" encoding="UTF-8"?><error><code>PermissionDenied</code><message>PermissionDenied</message></error>"#,
            ))
            .mount(&server)
            .await;

        let error = client
            .download_share_file(
                42,
                &File {
                    id: "file-1".into(),
                    name: "cas.json".into(),
                    size: 10,
                    md5: "md5".into(),
                },
            )
            .await
            .unwrap_err();

        match error {
            RequestError::Error(message) => assert!(message.contains("PermissionDenied")),
            other => panic!("expected permission error, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn download_share_file_requires_auth_config() {
        let server = MockServer::start().await;

        let error = unauthenticated_client(&server)
            .download_share_file(
                42,
                &File {
                    id: "file-1".into(),
                    name: "cas.json".into(),
                    size: 10,
                    md5: "md5".into(),
                },
            )
            .await
            .unwrap_err();

        match error {
            RequestError::Error(message) => {
                assert!(message.contains("pan189.username"));
            }
            other => panic!("expected business error, got {other:?}"),
        }
    }
}
