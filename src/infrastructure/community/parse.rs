use std::sync::LazyLock;

use regex::Regex;
use url::Url;

use crate::application::ports::CommunityThread;
use crate::infrastructure::share::is_supported_share_url;

static TID_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"data-tid="(\d+)""#).expect("tid regex"));
static TAG_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"class="badge[^"]*"[^>]*>([^<]+)</a>"#).expect("tag regex"));
static USERNAME_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"class="username[^"]*"[^>]*>([^<]+)"#).expect("username regex"));
static DATE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"class="date[^"]*"[^>]*>([^<]+)"#).expect("date regex"));
static COMMENTS_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"#comments"[^>]*>[\s\S]*?<span>\s*([0-9.]+k?)\s*</span>"#).expect("comments regex")
});
static LIKES_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"fa-thumbs-up[\s\S]*?<span>\s*([0-9.]+k?)\s*</span>"#).expect("likes regex")
});
static HREF_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"href=["'](https?://[^"']+)["']"#).expect("href regex"));
static URL_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"https?://[^\s<>"']+"#).expect("url regex"));
static PASSWORD_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?:提取码|密码|pwd)\s*[:：=]?\s*([A-Za-z0-9]{3,8})").expect("password regex")
});
static TAG_STRIP_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"<[^>]+>").expect("tag strip regex"));
static UID_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"var\s+uid\s*=\s*(\d+)").expect("uid regex"));

pub fn xn_urlencode(s: &str) -> String {
    let mut encoded = String::new();
    for ch in s.chars() {
        let keep = matches!(
            ch,
            'A'..='Z'
                | 'a'..='z'
                | '0'..='9'
                | '-'
                | '_'
                | '.'
                | '!'
                | '~'
                | '*'
                | '\''
                | '('
                | ')'
        );
        if keep {
            encoded.push(ch);
        } else {
            for b in ch.to_string().into_bytes() {
                encoded.push_str(&format!("%{b:02X}"));
            }
        }
    }
    encoded = encoded.replace('_', "%5f");
    encoded = encoded.replace('-', "%2d");
    encoded = encoded.replace('.', "%2e");
    encoded = encoded.replace('~', "%7e");
    encoded = encoded.replace('!', "%21");
    encoded = encoded.replace('*', "%2a");
    encoded = encoded.replace('(', "%28");
    encoded = encoded.replace(')', "%29");
    encoded.replace('%', "_")
}

pub fn parse_search_html(html: &str, base_url: &str) -> Vec<CommunityThread> {
    let tids: Vec<(i64, usize)> = TID_RE
        .captures_iter(html)
        .filter_map(|cap| {
            let tid = cap.get(1)?.as_str().parse().ok()?;
            Some((tid, cap.get(0)?.start()))
        })
        .collect();

    tids.iter()
        .enumerate()
        .filter_map(|(index, (tid, start))| {
            let end = tids
                .get(index + 1)
                .map(|(_, next)| *next)
                .unwrap_or(html.len());
            parse_thread_card(&html[*start..end], *tid, base_url)
        })
        .collect()
}

fn parse_thread_card(chunk: &str, tid: i64, base_url: &str) -> Option<CommunityThread> {
    let title_href = format!(r#"href="?thread-{tid}.htm""#);
    let title_start = chunk.find(&title_href)?;
    let after_href = &chunk[title_start + title_href.len()..];
    let inner_start = after_href.find('>')? + 1;
    let inner_end = after_href[inner_start..].find("</a>")?;
    let title = normalize_text(&after_href[inner_start..inner_start + inner_end]);
    if title.is_empty() {
        return None;
    }

    let subject_end = chunk.find(r#"class="d-flex"#).unwrap_or(chunk.len());
    let tags = TAG_RE
        .captures_iter(&chunk[..subject_end])
        .filter_map(|cap| {
            let tag = normalize_text(cap.get(1)?.as_str());
            (!tag.is_empty()).then_some(tag)
        })
        .collect();

    let author = USERNAME_RE
        .captures(chunk)
        .and_then(|cap| cap.get(1).map(|m| normalize_text(m.as_str())))
        .unwrap_or_default();
    let posted_at = DATE_RE
        .captures(chunk)
        .and_then(|cap| cap.get(1).map(|m| normalize_text(m.as_str())))
        .unwrap_or_default();
    let comments = COMMENTS_RE
        .captures(chunk)
        .and_then(|cap| parse_count(cap.get(1)?.as_str()))
        .unwrap_or(0);
    let likes = LIKES_RE
        .captures(chunk)
        .and_then(|cap| parse_count(cap.get(1)?.as_str()))
        .unwrap_or(0);

    Some(CommunityThread {
        tid,
        title,
        tags,
        author,
        posted_at,
        comments,
        likes,
        url: thread_url(base_url, tid),
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThreadPage {
    pub title: String,
    pub logged_in: bool,
    pub hidden: bool,
    pub share_urls: Vec<String>,
}

pub fn parse_thread_html(html: &str, tid: i64, _base_url: &str) -> ThreadPage {
    let logged_in = UID_RE
        .captures(html)
        .and_then(|cap| cap.get(1)?.as_str().parse::<i64>().ok())
        .is_some_and(|uid| uid > 0);

    let title = extract_thread_title(html, tid);
    let first_message = first_message_html(html);
    let hidden = first_message.contains("请回复后再查看");
    let share_urls = if hidden {
        Vec::new()
    } else {
        extract_share_urls(first_message)
    };

    ThreadPage {
        title,
        logged_in,
        hidden,
        share_urls,
    }
}

pub fn thread_url(base_url: &str, tid: i64) -> String {
    format!("{}/?thread-{tid}.htm", base_url.trim_end_matches('/'))
}

pub fn search_url(base_url: &str, keyword: &str) -> String {
    let encoded = xn_urlencode(keyword);
    format!("{}/?search-{encoded}-1.htm", base_url.trim_end_matches('/'))
}

fn extract_thread_title(html: &str, tid: i64) -> String {
    let marker = format!(r#"href="?thread-{tid}.htm""#);
    if let Some(pos) = html.find(&marker) {
        let after = &html[pos + marker.len()..];
        if let Some(inner_start) = after.find('>')
            && let Some(inner_end) = after[inner_start + 1..].find("</a>")
        {
            let title = normalize_text(&after[inner_start + 1..inner_start + 1 + inner_end]);
            if !title.is_empty() {
                return title;
            }
        }
    }
    html.find("<h4")
        .and_then(|start| {
            let slice = &html[start..];
            let inner_start = slice.find('>')? + 1;
            let inner_end = slice[inner_start..].find('<')?;
            Some(normalize_text(&slice[inner_start..inner_start + inner_end]))
        })
        .unwrap_or_else(|| format!("thread-{tid}"))
}

fn first_message_html(html: &str) -> &str {
    if let Some(attr_at) = html.find("isfirst=\"1\"") {
        let head = html[..attr_at].rfind("<div").unwrap_or(attr_at);
        let rest = &html[attr_at..];
        let end = rest
            .find("<ul class=\"list-unstyled")
            .or_else(|| rest.find("class=\"postlist\""))
            .unwrap_or(rest.len());
        return &html[head..attr_at + end];
    }
    if let Some(start) = html.find("class=\"message") {
        let head = html[..start].rfind("<div").unwrap_or(start);
        let rest = &html[start..];
        let end = rest.find("<ul class=\"list-unstyled").unwrap_or(rest.len());
        return &html[head..start + end];
    }
    html
}

fn extract_share_urls(html: &str) -> Vec<String> {
    let text = normalize_text(html);
    let password = PASSWORD_RE
        .captures(&text)
        .and_then(|cap| cap.get(1).map(|m| m.as_str().to_owned()));

    let mut urls = Vec::new();
    let mut seen = std::collections::HashSet::new();

    for cap in HREF_RE.captures_iter(html) {
        if let Some(raw) = cap.get(1) {
            push_share_url(&mut urls, &mut seen, raw.as_str(), password.as_deref());
        }
    }
    for cap in URL_RE.captures_iter(&text) {
        push_share_url(
            &mut urls,
            &mut seen,
            cap.get(0).unwrap().as_str(),
            password.as_deref(),
        );
    }
    urls
}

fn push_share_url(
    urls: &mut Vec<String>,
    seen: &mut std::collections::HashSet<String>,
    raw: &str,
    password: Option<&str>,
) {
    let decoded = html_unescape(raw).replace("&amp;", "&");
    let decoded = decoded.trim_end_matches(['.', ',', ';', ')', ']']);
    let Ok(mut url) = Url::parse(decoded) else {
        return;
    };
    if !is_supported_share_url(&url) {
        return;
    }
    if let Some(password) = password {
        let has_pwd = url
            .query_pairs()
            .any(|(key, _)| key == "pwd" || key == "password");
        if !has_pwd {
            url.query_pairs_mut().append_pair("pwd", password);
        }
    }
    let serialized = url.to_string();
    if seen.insert(serialized.clone()) {
        urls.push(serialized);
    }
}

fn parse_count(raw: &str) -> Option<u32> {
    let raw = raw.trim();
    if let Some(stripped) = raw.strip_suffix('k').or_else(|| raw.strip_suffix('K')) {
        let value: f32 = stripped.parse().ok()?;
        return Some((value * 1000.0) as u32);
    }
    raw.parse().ok()
}

fn normalize_text(html: &str) -> String {
    let stripped = TAG_STRIP_RE.replace_all(html, "");
    html_unescape(&stripped)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn html_unescape(value: &str) -> String {
    value
        .replace("&amp;", "&")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&nbsp;", " ")
}

#[cfg(test)]
mod tests {
    use super::*;

    const SEARCH_HTML: &str = r#"
<ul class="list-unstyled threadlist mb-0">
<li class="media thread tap " data-href="?thread-50570.htm" data-tid="50570">
  <div class="style3_subject break-all">
    <a href="?thread-50570.htm">欧美剧 《<span class="text-danger">黑镜</span> 1-7季》中文字幕</a>
    <a href="?forum-48-1.htm&tagids=24" class="badge badge-pill badge-dark">1080p</a>
    <a href="?forum-48-1.htm&tagids=109" class="badge badge-pill badge-success">完结</a>
  </div>
  <div class="d-flex justify-content-between small mt-1">
    <a href="?user-30648.htm" class="username text-muted mr-1" uid="30648">奶糖小兔</a>
    <span class="date text-grey hidden-sm">2026-01-06 10:13</span>
    <a href="?thread-50570.htm#comments" class="ml-2"><span> 104</span></a>
    <span class="far fa-thumbs-up"></span>
    <span>1</span>
  </div>
</li>
</ul>
"#;

    #[test]
    fn encodes_xiuno_search_keyword() {
        assert_eq!(xn_urlencode("黑镜"), "_E9_BB_91_E9_95_9C");
        assert_eq!(xn_urlencode("a_b"), "a_5fb");
        assert_eq!(
            search_url("https://pan1.me", "黑镜"),
            "https://pan1.me/?search-_E9_BB_91_E9_95_9C-1.htm"
        );
    }

    #[test]
    fn parses_search_thread_cards() {
        let threads = parse_search_html(SEARCH_HTML, "https://pan1.me");
        assert_eq!(threads.len(), 1);
        let thread = &threads[0];
        assert_eq!(thread.tid, 50570);
        assert_eq!(thread.title, "欧美剧 《黑镜 1-7季》中文字幕");
        assert_eq!(thread.tags, vec!["1080p", "完结"]);
        assert_eq!(thread.author, "奶糖小兔");
        assert_eq!(thread.posted_at, "2026-01-06 10:13");
        assert_eq!(thread.comments, 104);
        assert_eq!(thread.likes, 1);
        assert_eq!(thread.url, "https://pan1.me/?thread-50570.htm");
    }

    #[test]
    fn detects_hidden_thread_without_extracting_links() {
        let html = r#"
var uid = 25729;
<a href="?thread-50570.htm">黑镜</a>
<div class="message break-all" isfirst="1">
  <div class="alert alert-warning">您好，本帖含有特定内容，请回复后再查看。</div>
  <a href="https://www.123684.com/s/hiddenkey?pwd=pass">hidden</a>
</div>
<ul class="list-unstyled  postlist"></ul>
"#;
        let page = parse_thread_html(html, 50570, "https://pan1.me");
        assert!(page.logged_in);
        assert!(page.hidden);
        assert!(page.share_urls.is_empty());
        assert_eq!(page.title, "黑镜");
    }

    #[test]
    fn extracts_share_url_and_password_from_unlocked_post() {
        let html = r#"
var uid = 25729;
<a href="?thread-1.htm">测试</a>
<div class="message break-all" isfirst="1">
  <p>链接：<a href="https://www.123684.com/s/share-key">https://www.123684.com/s/share-key</a></p>
  <p>提取码：pass</p>
</div>
"#;
        let page = parse_thread_html(html, 1, "https://pan1.me");
        assert!(page.logged_in);
        assert!(!page.hidden);
        assert_eq!(
            page.share_urls,
            vec!["https://www.123684.com/s/share-key?pwd=pass".to_string()]
        );
    }

    #[test]
    fn treats_uid_zero_as_logged_out() {
        let html = "var uid = 0;\n<div class=\"message\">hello</div>";
        let page = parse_thread_html(html, 1, "https://pan1.me");
        assert!(!page.logged_in);
    }
}
