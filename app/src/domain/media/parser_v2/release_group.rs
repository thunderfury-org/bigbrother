use std::ops::Range;
use std::sync::LazyLock;

use regex::Regex;

use super::super::normalize::normalize_quality;

static TECHNICAL_GROUP_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?ix)
        ^(?:WEB|DL|DV|HDR|HDR10|HDR10\+|AAC|DD|DDP|DTS|DTS-HD|HD|MA|TRUEHD|ATMOS|HEVC|AVC|H264|H265|REMUX|BLURAY|HQ|\d{2,3}FPS|\d+)$
        ",
    )
    .unwrap()
});

static TECHNICAL_FRAGMENT_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?ix)
        (?:WEB|WEB-?DL|WEB-?RIP|WEBRIP|BLU-?RAY|REMUX|BD-?RIP|BR-?RIP|HEVC|AVC|AV1|VP-9|H\.?26[45]|X26[45]
        |AAC|FLAC|DTS(?:-?HD)?|TRUEHD|ATMOS|HDR10\+?|HDR|DV|DOVI|HLG
        |\d{2,3}FPS|4K|\d{3,4}P|\d{3,4}X\d{3,4}|\d+(?:\.\d+)?G(?:B)?|[A-F0-9]{8}|MKV|MP4|SRT|ASS|SSA|PGS
        |CHS(?:[._-]?JP)?|CHT|BIG5|ZH-HANS|ZH-HANT|JP|简中|繁中|簡中|簡體|繁體|简体|繁体|简繁日多语|字幕|内封|外挂字幕
        |UHD|Ultra\s+HD|SDR|EXTENDED|P\d+|HQ|Baha|B-Global|ViuTV|OVA|OAD|NCOP|NCED|Fin|附外挂字幕|招募翻译校对|日語原聲|日文自動產生字幕|进化版|Web先行版|先行版|英语中字|蓝光原盘)
        ",
    )
    .unwrap()
});

static NOISE_BRACKET_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?ix)
        ^(?:WEB|WEB-?DL|WEB-?RIP|WEBRIP|BLU-?RAY|REMUX|BD-?RIP|BR-?RIP|HEVC|AVC|AV1|VP-9|H\.?26[45]|X26[45]
        |AAC|FLAC|DTS(?:-?HD)?|TRUEHD|ATMOS|HDR10\+?|HDR|DV|DOVI|HLG
        |\d{2,3}FPS|4K|\d{3,4}P|\d{3,4}X\d{3,4}|\d+(?:\.\d+)?G(?:B)?|[A-F0-9]{8}|MKV|MP4|SRT|ASS|SSA|PGS
        |CHS(?:[._-]?JP)?|CHT|BIG5|ZH-HANS|ZH-HANT|JP|简中|繁中|簡中|簡體|繁體|简体|繁体|简繁内封|简繁日内封字幕|简日双语MP4/繁日双语MP4/简繁日多语MKV
        |字幕|内封|外挂字幕|附外挂字幕|招募翻译校对|日語原聲|日文自動產生字幕|OVA|OAD|NCOP|NCED|Fin
        |UHD|Ultra\s+HD|SDR|EXTENDED|P\d+|HQ|Baha|B-Global|ViuTV)$",
    )
    .unwrap()
});

pub(crate) fn looks_like_release_group(value: &str, is_noise_bracket: fn(&str) -> bool) -> bool {
    if value.to_ascii_lowercase().contains("tmdb") {
        return false;
    }
    if value.contains('第') || value.contains('集') {
        return false;
    }
    if value.contains('/') {
        return false;
    }
    if value
        .chars()
        .all(|ch| ch.is_ascii_digit() || matches!(ch, '-' | '~' | ' '))
    {
        return false;
    }
    if value.is_empty() || is_noise_bracket(value) {
        return false;
    }

    let lower = value.to_ascii_lowercase();
    value.contains('-')
        || value.contains('&')
        || value.contains("字幕组")
        || value.contains("Raws")
        || value.contains("raws")
        || value.contains("Asia")
        || value.contains("个人翻译")
        || value.contains("制作")
        || (value.contains(' ')
            && (lower == value
                || lower.contains("raws")
                || lower.contains("sub")
                || lower.contains("team")
                || lower.contains("group")))
        || (value.len() <= 20
            && value.chars().any(|ch| ch.is_ascii_alphabetic())
            && !value.contains(' '))
}

pub(crate) fn is_noise_bracket(value: &str) -> bool {
    let value = value.trim();
    if value.is_empty() {
        return true;
    }
    if NOISE_BRACKET_RE.is_match(value) {
        return true;
    }
    if value.contains("字幕")
        && !value.contains("字幕组")
        && !value.contains("字幕社")
        && !value.contains("汉化组")
    {
        return true;
    }

    is_metadata_fragment(value)
}

pub(crate) fn is_metadata_fragment(value: &str) -> bool {
    let stripped = TECHNICAL_FRAGMENT_RE.replace_all(value, "");
    stripped
        .chars()
        .all(|ch| matches!(ch, ' ' | '.' | '-' | '_' | '/' | '@' | '&'))
}

pub(crate) fn technical_fragment_spans(body: &str) -> Vec<Range<usize>> {
    TECHNICAL_FRAGMENT_RE
        .find_iter(body)
        .map(|m| m.range())
        .collect()
}

pub(crate) fn trailing_release_group(
    body: &str,
    occupied: &[bool],
    is_metadata_fragment: fn(&str) -> bool,
    is_noise_bracket: fn(&str) -> bool,
) -> Option<(Range<usize>, String)> {
    trailing_release_group_with_delimiter(
        body,
        occupied,
        '-',
        is_metadata_fragment,
        is_noise_bracket,
    )
    .or_else(|| {
        trailing_release_group_with_delimiter(
            body,
            occupied,
            '@',
            is_metadata_fragment,
            is_noise_bracket,
        )
    })
}

fn trailing_release_group_with_delimiter(
    body: &str,
    occupied: &[bool],
    delimiter: char,
    is_metadata_fragment: fn(&str) -> bool,
    is_noise_bracket: fn(&str) -> bool,
) -> Option<(Range<usize>, String)> {
    let delimiter_index = body.rfind(delimiter)?;
    let candidate = body[delimiter_index + delimiter.len_utf8()..].trim();
    if candidate.is_empty()
        || !is_release_group_candidate(candidate, is_metadata_fragment, is_noise_bracket)
    {
        return None;
    }
    if !has_recent_occupied_before(occupied, delimiter_index, 32) {
        return None;
    }
    let prefix_token = body[..delimiter_index]
        .rsplit(['.', ' ', '_', '-', '[', ']', '(', ')'])
        .next()
        .unwrap_or_default()
        .trim();
    if delimiter == '-'
        && prefix_token.eq_ignore_ascii_case("dolby")
        && candidate.eq_ignore_ascii_case("vision")
    {
        return None;
    }

    Some((delimiter_index..body.len(), candidate.to_owned()))
}

fn is_release_group_candidate(
    value: &str,
    is_metadata_fragment: fn(&str) -> bool,
    is_noise_bracket: fn(&str) -> bool,
) -> bool {
    if value.to_ascii_lowercase().contains("tmdb") {
        return false;
    }
    if value.contains('第') || value.contains('集') {
        return false;
    }
    if value
        .chars()
        .any(|ch| matches!(ch, '[' | ']' | '{' | '}' | '/'))
    {
        return false;
    }
    if looks_like_technical_group(value) {
        return false;
    }
    if is_metadata_fragment(value) {
        return false;
    }
    if value
        .chars()
        .all(|ch| ch.is_ascii_digit() || matches!(ch, '-' | '~' | ' '))
    {
        return false;
    }
    if is_noise_bracket(value) {
        return false;
    }

    !matches!(
        normalize_quality(value).as_str(),
        "WEB-DL" | "WEBRip" | "BluRay" | "Remux" | "BDRip" | "BRRip"
    )
}

fn looks_like_technical_group(value: &str) -> bool {
    let segments = value
        .split(|ch: char| ch == '.' || ch.is_whitespace())
        .map(str::trim)
        .filter(|segment| !segment.is_empty())
        .collect::<Vec<_>>();
    if segments.is_empty() {
        return false;
    }

    segments.iter().all(|segment| {
        let upper = segment.to_ascii_uppercase();
        TECHNICAL_GROUP_RE.is_match(upper.as_str())
            || upper.starts_with("AAC")
            || upper.starts_with("DDP")
            || upper.starts_with("DTS")
            || upper.starts_with("TRUEHD")
            || segment.parse::<u32>().is_ok()
    })
}

fn has_recent_occupied_before(occupied: &[bool], index: usize, window: usize) -> bool {
    let window_start = index.saturating_sub(window);
    occupied
        .get(window_start..index)
        .is_some_and(|slice| slice.iter().any(|used| *used))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn never_noise(_: &str) -> bool {
        false
    }

    fn tech_noise(value: &str) -> bool {
        matches!(value, "WEB-DL" | "HDR" | "AAC")
    }

    fn metadata_fragment(value: &str) -> bool {
        matches!(value, "WEB.DL" | "AAC.2.0" | "HDR")
    }

    #[test]
    fn accepts_common_release_group_markers() {
        assert!(looks_like_release_group("BTN", never_noise));
        assert!(looks_like_release_group(
            "爱恋字幕社&猫恋汉化组",
            never_noise
        ));
        assert!(looks_like_release_group("智械尚未危机制作", never_noise));
    }

    #[test]
    fn rejects_obvious_non_groups() {
        assert!(!looks_like_release_group("WEB-DL", tech_noise));
        assert!(!looks_like_release_group("2024", never_noise));
        assert!(!looks_like_release_group("第01集", never_noise));
    }

    #[test]
    fn detects_trailing_release_group_after_technical_context() {
        let body = "Perfect.Crown.S01E02.1080p.WEB-DL.H264-BTN";
        let mut occupied = vec![false; body.len()];
        let marker = body.find("1080p.WEB-DL.H264").unwrap();
        occupied[marker..marker + "1080p.WEB-DL.H264".len()].fill(true);

        let (_, value) =
            trailing_release_group(body, &occupied, metadata_fragment, tech_noise).unwrap();
        assert_eq!(value, "BTN");
    }

    #[test]
    fn rejects_dolby_vision_false_positive() {
        let body = "Movie.Dolby-Vision";
        let occupied = vec![true; body.find('-').unwrap()];

        assert!(trailing_release_group(body, &occupied, metadata_fragment, tech_noise).is_none());
    }
}
