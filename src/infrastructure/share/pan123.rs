use url::Url;

use crate::{
    domain::share::RawFile,
    error::{AppError, AppResult},
    infrastructure::client::pan123,
};

use super::{collect::collect_pan123_directory_entries, traversal::ShareTraversal};

pub(crate) fn parse_share_parts(url: &Url) -> Option<(String, String)> {
    if !(url.host_str().is_some_and(|host| {
        (host.starts_with("www.123") || host.contains(".share.123") || host.contains(".mshare.123"))
            && (host.ends_with(".com") || host.ends_with(".cn"))
    }) && (url.path().starts_with("/s/") || url.path().starts_with("/123")))
    {
        return None;
    }

    let share_key = url
        .path_segments()
        .map(|mut segments| segments.next_back().unwrap_or_default())
        .unwrap_or_default()
        .to_owned();
    if share_key.is_empty() {
        return None;
    }

    let share_password = url
        .query_pairs()
        .find(|(key, _)| key == "pwd")
        .map(|(_, value)| value.to_string())
        .unwrap_or_default();
    Some((share_key, share_password))
}

#[derive(Clone)]
pub struct Pan123ShareService<S> {
    share_source: S,
}

pub(crate) trait Pan123ShareSource: Clone {
    fn list_share_files(
        &self,
        share_key: &str,
        share_password: &str,
        parent_id: i64,
    ) -> impl std::future::Future<Output = AppResult<Vec<pan123::File>>> + Send;
}

impl Pan123ShareSource for pan123::Client {
    async fn list_share_files(
        &self,
        share_key: &str,
        share_password: &str,
        parent_id: i64,
    ) -> AppResult<Vec<pan123::File>> {
        Ok(self
            .list_share_files(share_key, share_password, parent_id)
            .await?)
    }
}

impl<S: Pan123ShareSource> Pan123ShareService<S> {
    pub fn new(share_source: S) -> Self {
        Self { share_source }
    }

    pub async fn raw_files_from_share(
        &self,
        share_key: &str,
        share_password: &str,
    ) -> AppResult<Vec<RawFile>> {
        if share_key.is_empty() {
            return Err(AppError::NotFound(
                "Can not extract share key from URL".into(),
            ));
        }

        let mut traversal = ShareTraversal::new((0, String::new()));

        while let Some((parent_id, parent_path)) = traversal.next_dir() {
            let files = self
                .share_source
                .list_share_files(share_key, share_password, parent_id)
                .await?;
            traversal.extend(collect_pan123_directory_entries(&files, &parent_path));
        }

        Ok(traversal.into_raw_files())
    }
}

#[cfg(test)]
mod tests {
    use url::Url;

    use super::parse_share_parts;

    #[test]
    fn matches_supported_pan123_urls_and_parses_share_parts() {
        let url = Url::parse("https://www.123684.com/s/share-key?pwd=pass").unwrap();

        assert_eq!(
            parse_share_parts(&url),
            Some(("share-key".into(), "pass".into()))
        );
    }

    #[test]
    fn parses_share_123865_subdomain_url() {
        let url = Url::parse("https://1846369609.share.123pan.cn/123pan/key?pwd=pass#").unwrap();

        assert_eq!(parse_share_parts(&url), Some(("key".into(), "pass".into())));
    }
}
