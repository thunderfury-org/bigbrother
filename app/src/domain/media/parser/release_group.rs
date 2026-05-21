use std::ops::Range;

use super::super::Metadata;
use super::super::normalize::normalize_quality;
use super::labels::{Label, NOISE_BRACKET_RE, TECHNICAL_FRAGMENT_RE, TECHNICAL_GROUP_RE};
use super::spans_overlap;
use super::tokenizer::{Token, TokenKind};

pub(super) fn extract_release_group(body: &str, tokens: &mut [Token], metadata: &mut Metadata) {
    // First pass: leading bracket. Accept a non-classifying first bracket as
    // release group if (a) its content qualifies, OR (b) the trailing body
    // starts with the promotional '★' marker.
    for (idx, token) in tokens.iter_mut().enumerate() {
        if !matches!(token.kind, TokenKind::Bracketed) {
            continue;
        }
        let value = token.text.trim();
        if value.is_empty() || is_noise_bracket(value) {
            token.label = Label::PromotionalNoise;
            continue;
        }
        let promotes_via_star = idx == 0 && {
            let after = body[token.span.end..].trim_start_matches(']');
            promotional_prefix_after_leading_bracket(after)
        };
        if looks_like_release_group(value, is_noise_bracket) || promotes_via_star {
            metadata.release_group = value.to_owned();
            token.label = Label::Group;
            break;
        }
    }

    if metadata.release_group.is_empty() {
        // Second pass: any later bracket that classifies.
        for token in tokens.iter_mut() {
            if !matches!(token.kind, TokenKind::Bracketed) {
                continue;
            }
            if !matches!(token.label, Label::Unknown) {
                continue;
            }
            let value = token.text.trim();
            if looks_like_release_group(value, is_noise_bracket) {
                metadata.release_group = value.to_owned();
                token.label = Label::Group;
                break;
            }
        }
    }

    if metadata.release_group.is_empty()
        && let Some((span, group)) =
            trailing_release_group(body, tokens, is_metadata_fragment, is_noise_bracket)
    {
        metadata.release_group = group;
        for token in tokens.iter_mut() {
            if spans_overlap(&token.span, &span) {
                token.label = Label::Group;
            }
        }
    }
}

fn promotional_prefix_after_leading_bracket(after: &str) -> bool {
    let trimmed = after.trim_start();
    if trimmed.starts_with('★') {
        return true;
    }
    let Some(rest) = trimmed.strip_prefix('.') else {
        return false;
    };
    matches!(rest.trim_start().chars().next(), Some('[' | '('))
}

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

pub(crate) fn trailing_release_group(
    body: &str,
    tokens: &[Token],
    is_metadata_fragment: fn(&str) -> bool,
    is_noise_bracket: fn(&str) -> bool,
) -> Option<(Range<usize>, String)> {
    trailing_release_group_with_delimiter(body, tokens, '-', is_metadata_fragment, is_noise_bracket)
        .or_else(|| {
            trailing_release_group_with_delimiter(
                body,
                tokens,
                '@',
                is_metadata_fragment,
                is_noise_bracket,
            )
        })
}

fn trailing_release_group_with_delimiter(
    body: &str,
    tokens: &[Token],
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
    if !preceding_token_is_classified(tokens, delimiter_index) {
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

fn preceding_token_is_classified(tokens: &[Token], byte_index: usize) -> bool {
    // The trailing release-group heuristic only fires when the delimiter sits
    // immediately after technical metadata. With token labels available, that
    // is precisely "the nearest token ending at or before this byte index has
    // a label other than Title/Unknown".
    tokens
        .iter()
        .rfind(|token| token.span.end <= byte_index)
        .map(|token| !matches!(token.label, Label::Title | Label::Unknown))
        .unwrap_or(false)
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
        let tokens = vec![
            Token {
                span: 0..body.find('-').unwrap(),
                text: body[..body.find('-').unwrap()].to_owned(),
                kind: super::super::tokenizer::TokenKind::Bare,
                label: Label::VideoCodec,
            },
            Token {
                span: body.find('-').unwrap() + 1..body.len(),
                text: "BTN".to_owned(),
                kind: super::super::tokenizer::TokenKind::Bare,
                label: Label::Unknown,
            },
        ];

        let (_, value) =
            trailing_release_group(body, &tokens, metadata_fragment, tech_noise).unwrap();
        assert_eq!(value, "BTN");
    }

    #[test]
    fn rejects_dolby_vision_false_positive() {
        let body = "Movie.Dolby-Vision";
        let dash = body.find('-').unwrap();
        let tokens = vec![
            Token {
                span: 0..dash,
                text: body[..dash].to_owned(),
                kind: super::super::tokenizer::TokenKind::Bare,
                label: Label::Hdr,
            },
            Token {
                span: dash + 1..body.len(),
                text: "Vision".to_owned(),
                kind: super::super::tokenizer::TokenKind::Bare,
                label: Label::Unknown,
            },
        ];

        assert!(trailing_release_group(body, &tokens, metadata_fragment, tech_noise).is_none());
    }
}
