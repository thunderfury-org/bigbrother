use url::Url;

use crate::{
    domain::share::RawFile,
    error::{AppError, AppResult},
};

use super::{ShareClient, collect::collect_pan115_directory_entries, traversal::ShareTraversal};

pub(crate) fn match_url(url: &Url) -> bool {
    url.host_str()
        .is_some_and(|host| host == "115.com" || host == "115cdn.com")
        && url.path().starts_with("/s/")
}

pub(crate) fn parse_share_parts(url: &Url) -> Option<(String, String)> {
    if !match_url(url) {
        return None;
    }

    let share_code = path_segment_after_prefix(url, 1);
    if share_code.is_empty() {
        return None;
    }

    let receive_code = url
        .query_pairs()
        .find(|(key, _)| key == "password" || key == "rc")
        .map(|(_, value)| value.to_string())
        .unwrap_or_default();
    Some((share_code, receive_code))
}

#[derive(Clone)]
pub struct Pan115ShareService<S> {
    share_source: S,
}

impl<S: ShareClient> Pan115ShareService<S> {
    pub fn new(share_source: S) -> Self {
        Self { share_source }
    }

    pub async fn raw_files_from_share(
        &self,
        share_code: &str,
        receive_code: &str,
    ) -> AppResult<Vec<RawFile>> {
        if share_code.is_empty() {
            return Err(AppError::NotFound(
                "Can not extract share code from URL".into(),
            ));
        }

        let mut traversal = ShareTraversal::new(("0".to_string(), String::new()));

        while let Some((cid, parent_path)) = traversal.next_dir() {
            let entries = self
                .share_source
                .list_pan115_share_files(share_code, receive_code, &cid)
                .await?;
            traversal.extend(collect_pan115_directory_entries(&entries, &parent_path));
        }

        Ok(traversal.into_raw_files())
    }
}

fn path_segment_after_prefix(url: &Url, index: usize) -> String {
    url.path()
        .strip_prefix('/')
        .and_then(|path| path.split('/').nth(index))
        .unwrap_or_default()
        .to_owned()
}

#[cfg(test)]
mod tests {
    use url::Url;

    use super::{match_url, parse_share_parts};

    #[test]
    fn matches_supported_pan115_urls_and_parses_share_parts() {
        let url = Url::parse("https://115.com/s/share115?rc=recv").unwrap();

        assert!(match_url(&url));
        assert_eq!(
            parse_share_parts(&url),
            Some(("share115".into(), "recv".into()))
        );
    }

    #[test]
    fn accepts_pan115_url_shape_but_rejects_missing_share_code() {
        let url = Url::parse("https://115.com/s/").unwrap();

        assert!(match_url(&url));
        assert_eq!(parse_share_parts(&url), None);
    }
}
