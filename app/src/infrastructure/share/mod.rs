mod collect;
pub mod file_parser;
pub mod pan115;
pub mod pan123;
pub mod pan189;
pub mod quark;
pub mod resolver;
mod traversal;

pub fn is_supported_share_url(url: &url::Url) -> bool {
    pan123::parse_share_parts(url).is_some()
        || pan189::parse_share_code(url).is_some()
        || pan115::parse_share_parts(url).is_some()
        || quark::parse_share_parts(url).is_some()
}

#[cfg(test)]
mod tests {
    use url::Url;

    use super::is_supported_share_url;

    #[test]
    fn identifies_supported_share_urls() {
        assert!(is_supported_share_url(
            &Url::parse("https://pan.quark.cn/s/share-id?pwd=abc").unwrap()
        ));
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
}
