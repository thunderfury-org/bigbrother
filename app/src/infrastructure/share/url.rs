use url::Url;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ShareUrl<'a> {
    Pan123(&'a Url),
    Pan189(&'a Url),
    Pan115(&'a Url),
    Quark(&'a Url),
}

pub(crate) fn parse_share_url(url: &Url) -> Option<ShareUrl<'_>> {
    if url.host_str().is_some_and(is_pan123_host)
        && is_pan123_share_path(url)
        && !parse_pan123_share_parts(url).0.is_empty()
    {
        Some(ShareUrl::Pan123(url))
    } else if url.host_str().is_some_and(|host| host == "cloud.189.cn")
        && (url.path().starts_with("/t/")
            || (url.path() == "/web/share"
                && url
                    .query_pairs()
                    .any(|(key, value)| key == "code" && !value.is_empty())))
        && !parse_pan189_share_code(url).is_empty()
    {
        Some(ShareUrl::Pan189(url))
    } else if url
        .host_str()
        .is_some_and(|host| host == "115.com" || host == "115cdn.com")
        && url.path().starts_with("/s/")
        && !parse_pan115_share_parts(url).0.is_empty()
    {
        Some(ShareUrl::Pan115(url))
    } else if url.host_str().is_some_and(|host| host == "pan.quark.cn")
        && url.path().starts_with("/s/")
        && !parse_quark_share_parts(url).0.is_empty()
    {
        Some(ShareUrl::Quark(url))
    } else {
        None
    }
}

pub(crate) fn is_supported_share_url(url: &Url) -> bool {
    parse_share_url(url).is_some()
}

pub(crate) fn is_pan123_host(host: &str) -> bool {
    host == "www.123pan.com" || host == "www.123684.com" || host.ends_with(".share.123865.com")
}

fn is_pan123_share_path(url: &Url) -> bool {
    let Some(host) = url.host_str() else {
        return false;
    };

    if host == "www.123pan.com" || host == "www.123684.com" {
        return url.path().starts_with("/s/");
    }

    host.ends_with(".share.123865.com") && url.path().starts_with("/123pan/")
}

pub(crate) fn parse_pan123_share_parts(url: &Url) -> (String, String) {
    let share_key = if url.path().starts_with("/s/") || url.path().starts_with("/123pan/") {
        path_segment_after_prefix(url, 1)
    } else {
        String::new()
    };
    let share_password = url
        .query_pairs()
        .find(|(key, _)| key == "pwd")
        .map(|(_, value)| value.to_string())
        .unwrap_or_default();
    (share_key, share_password)
}

pub(crate) fn parse_pan189_share_code(url: &Url) -> String {
    url.query_pairs()
        .find(|(key, value)| key == "code" && !value.is_empty())
        .map(|(_, value)| value.to_string())
        .unwrap_or_else(|| {
            if url.path().starts_with("/t/") {
                path_segment_after_prefix(url, 1)
            } else {
                String::new()
            }
        })
}

pub(crate) fn parse_pan115_share_parts(url: &Url) -> (String, String) {
    let share_code = if url.path().starts_with("/s/") {
        path_segment_after_prefix(url, 1)
    } else {
        String::new()
    };
    let receive_code = url
        .query_pairs()
        .find(|(key, _)| key == "password" || key == "rc")
        .map(|(_, value)| value.to_string())
        .unwrap_or_default();
    (share_code, receive_code)
}

pub(crate) fn parse_quark_share_parts(url: &Url) -> (String, String) {
    let share_id = if url.path().starts_with("/s/") {
        path_segment_after_prefix(url, 1)
    } else {
        String::new()
    };
    let password = url
        .query_pairs()
        .find(|(key, _)| key == "pwd")
        .map(|(_, value)| value.to_string())
        .unwrap_or_default();
    (share_id, password)
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
    use super::*;

    #[test]
    fn share_url_recognizes_pan115_links() {
        for url in [
            "https://115cdn.com/s/swfoexi3no3?password=j7b2",
            "https://115.com/s/swfoexi3no3?rc=j7b2",
            "https://115.com/s/swfoexi3no3",
        ] {
            let parsed_url = Url::parse(url).unwrap();
            let parsed = parse_share_url(&parsed_url);
            assert!(parsed.is_some());
            assert!(matches!(parsed.unwrap(), ShareUrl::Pan115(_)));
        }
    }

    #[test]
    fn share_url_recognizes_supported_hosts() {
        let pan123 = Url::parse("https://www.123pan.com/s/abc123?pwd=test").unwrap();
        let pan123_alt = Url::parse("https://www.123684.com/s/abc123?pwd=test").unwrap();
        let pan123_new =
            Url::parse("https://1850081502.share.123865.com/123pan/4Ulmvd-hWbSA?pwd=33Rw").unwrap();
        let pan189 = Url::parse("https://cloud.189.cn/t/abc123").unwrap();
        let pan189_query = Url::parse("https://cloud.189.cn/web/share?code=abc123").unwrap();

        assert!(matches!(
            parse_share_url(&pan123),
            Some(ShareUrl::Pan123(_))
        ));
        assert!(matches!(
            parse_share_url(&pan123_alt),
            Some(ShareUrl::Pan123(_))
        ));
        assert!(matches!(
            parse_share_url(&pan123_new),
            Some(ShareUrl::Pan123(_))
        ));
        assert!(matches!(
            parse_share_url(&pan189),
            Some(ShareUrl::Pan189(_))
        ));
        assert!(matches!(
            parse_share_url(&pan189_query),
            Some(ShareUrl::Pan189(_))
        ));
    }

    #[test]
    fn share_url_recognizes_quark_links() {
        for url in [
            "https://pan.quark.cn/s/c094a3711bcc?pwd=67e5",
            "https://pan.quark.cn/s/c094a3711bcc",
        ] {
            let parsed_url = Url::parse(url).unwrap();
            let parsed = parse_share_url(&parsed_url);
            assert!(parsed.is_some());
            assert!(matches!(parsed.unwrap(), ShareUrl::Quark(_)));
        }
    }

    #[test]
    fn share_url_rejects_pan123_lookalike_hosts() {
        for url in [
            "https://www.123evil.com/s/demo",
            "https://www.123abc.com/s/demo",
            "https://share.123evil.com/123pan/demo",
            "https://1850081502.share.123865.com/123evil/demo",
        ] {
            let parsed_url = Url::parse(url).unwrap();
            assert!(parse_share_url(&parsed_url).is_none());
        }
    }

    #[test]
    fn share_url_rejects_pan189_web_share_without_code() {
        let url = Url::parse("https://cloud.189.cn/web/share").unwrap();

        assert!(parse_share_url(&url).is_none());
    }

    #[test]
    fn share_url_rejects_empty_share_identifiers() {
        for url in [
            "https://cloud.189.cn/web/share?code=",
            "https://cloud.189.cn/t/",
            "https://cloud.189.cn/t//abc",
            "https://www.123pan.com/s/",
            "https://www.123pan.com/s//abc",
            "https://www.123684.com/s/",
            "https://1850081502.share.123865.com/123pan/",
            "https://1850081502.share.123865.com/123pan//abc",
            "https://115.com/s/",
            "https://115.com/s//abc",
            "https://pan.quark.cn/s/",
            "https://pan.quark.cn/s//abc",
        ] {
            let parsed_url = Url::parse(url).unwrap();
            assert!(parse_share_url(&parsed_url).is_none(), "{url}");
        }
    }

    #[test]
    fn parses_share_parts_from_supported_urls() {
        let pan123 = Url::parse("https://www.123pan.com/s/share123?pwd=pass456").unwrap();
        let pan123_trailing = Url::parse("https://www.123pan.com/s/share123/?pwd=pass456").unwrap();
        let pan123_new =
            Url::parse("https://1850081502.share.123865.com/123pan/4Ulmvd-hWbSA?pwd=33Rw").unwrap();
        let pan189_query = Url::parse("https://cloud.189.cn/web/share?code=abc123").unwrap();
        let pan189_path_with_empty_query =
            Url::parse("https://cloud.189.cn/t/pathcode?code=").unwrap();
        let pan189_path = Url::parse("https://cloud.189.cn/t/pathcode").unwrap();
        let pan189_trailing = Url::parse("https://cloud.189.cn/t/pathcode/").unwrap();
        let pan115 = Url::parse("https://115.com/s/share115?password=recv").unwrap();
        let pan115_trailing = Url::parse("https://115.com/s/share115/?password=recv").unwrap();
        let pan115_rc = Url::parse("https://115.com/s/share115?rc=recv").unwrap();
        let quark_trailing = Url::parse("https://pan.quark.cn/s/c094a3711bcc/?pwd=abc123").unwrap();

        assert_eq!(
            parse_pan123_share_parts(&pan123),
            ("share123".into(), "pass456".into())
        );
        assert_eq!(
            parse_pan123_share_parts(&pan123_trailing),
            ("share123".into(), "pass456".into())
        );
        assert_eq!(
            parse_pan123_share_parts(&pan123_new),
            ("4Ulmvd-hWbSA".into(), "33Rw".into())
        );
        assert_eq!(parse_pan189_share_code(&pan189_query), "abc123");
        assert_eq!(
            parse_pan189_share_code(&pan189_path_with_empty_query),
            "pathcode"
        );
        assert_eq!(parse_pan189_share_code(&pan189_path), "pathcode");
        assert_eq!(parse_pan189_share_code(&pan189_trailing), "pathcode");
        assert_eq!(
            parse_pan115_share_parts(&pan115),
            ("share115".into(), "recv".into())
        );
        assert_eq!(
            parse_pan115_share_parts(&pan115_trailing),
            ("share115".into(), "recv".into())
        );
        assert_eq!(
            parse_pan115_share_parts(&pan115_rc),
            ("share115".into(), "recv".into())
        );
        assert_eq!(
            parse_quark_share_parts(&quark_trailing),
            ("c094a3711bcc".into(), "abc123".into())
        );
    }
}
