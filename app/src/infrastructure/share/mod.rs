mod collect;
pub mod file_parser;
pub mod pan115;
pub mod pan123;
pub mod pan189;
pub mod resolver;
mod traversal;

use crate::domain::import_record::ImportSourceKind;

pub fn share_provider_kind(url: &url::Url) -> Option<ImportSourceKind> {
    if pan123::parse_share_parts(url).is_some() {
        Some(ImportSourceKind::Pan123)
    } else if pan189::parse_share_code(url).is_some() {
        Some(ImportSourceKind::Pan189)
    } else if pan115::parse_share_parts(url).is_some() {
        Some(ImportSourceKind::Pan115)
    } else {
        None
    }
}

pub fn is_supported_share_url(url: &url::Url) -> bool {
    share_provider_kind(url).is_some()
}

#[cfg(test)]
mod tests {
    use url::Url;

    use super::{is_supported_share_url, share_provider_kind};
    use crate::domain::import_record::ImportSourceKind;

    #[test]
    fn identifies_supported_share_urls() {
        assert!(is_supported_share_url(
            &Url::parse("https://cloud.189.cn/t/share189").unwrap()
        ));
        assert!(is_supported_share_url(
            &Url::parse("https://115.com/s/share115?rc=recv").unwrap()
        ));
        assert!(is_supported_share_url(
            &Url::parse("https://www.123684.com/s/share-key?pwd=pass").unwrap()
        ));
    }

    #[test]
    fn rejects_unsupported_urls() {
        assert!(!is_supported_share_url(
            &Url::parse("https://www.themoviedb.org/tv/314784").unwrap()
        ));
    }

    #[test]
    fn share_provider_kind_maps_each_supported_provider() {
        assert_eq!(
            share_provider_kind(&Url::parse("https://cloud.189.cn/t/share189").unwrap()),
            Some(ImportSourceKind::Pan189)
        );
        assert_eq!(
            share_provider_kind(&Url::parse("https://115.com/s/share115?rc=recv").unwrap()),
            Some(ImportSourceKind::Pan115)
        );
        assert_eq!(
            share_provider_kind(
                &Url::parse("https://www.123684.com/s/share-key?pwd=pass").unwrap()
            ),
            Some(ImportSourceKind::Pan123)
        );
        assert_eq!(
            share_provider_kind(&Url::parse("https://example.com/share").unwrap()),
            None
        );
    }
}
