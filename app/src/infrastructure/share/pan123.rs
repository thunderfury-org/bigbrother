use url::Url;

use crate::{domain::share::RawFile, error::AppResult};

use super::{
    ShareClient,
    collect::collect_pan123_directory_entries,
    traversal::ShareTraversal,
    url::{ShareUrl, parse_pan123_share_parts, parse_share_url},
};

#[derive(Clone)]
pub struct Pan123ShareService<S> {
    share_source: S,
}

impl<S: ShareClient> Pan123ShareService<S> {
    pub fn new(share_source: S) -> Self {
        Self { share_source }
    }

    pub async fn raw_files_from_url(&self, url: &Url) -> AppResult<Vec<RawFile>> {
        let Some(ShareUrl::Pan123(url)) = parse_share_url(url) else {
            return Err(crate::error::AppError::InvalidParameter(format!(
                "unsupported pan123 share url: {url}"
            )));
        };
        let (share_key, share_password) = parse_pan123_share_parts(url);
        let mut traversal = ShareTraversal::new((0, String::new()));

        while let Some((parent_id, parent_path)) = traversal.next_dir() {
            let files = self
                .share_source
                .list_pan123_share_files(share_key.as_str(), share_password.as_str(), parent_id)
                .await?;
            traversal.extend(collect_pan123_directory_entries(&files, &parent_path));
        }

        Ok(traversal.into_raw_files())
    }
}
