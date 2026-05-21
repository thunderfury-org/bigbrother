//! Token-based parser pipeline (issue #81).
//!
//! Tokenises the filename once, classifies each token via the `labels`
//! dictionary plus small per-token pattern matchers, then walks the labeled
//! token stream to fill in `Metadata`. Replaces the historical byte-mask /
//! whole-string regex sweep approach.

use std::collections::HashSet;
use std::sync::LazyLock;

use lingua::{Language, LanguageDetector, LanguageDetectorBuilder};
use regex::Regex;

use super::super::normalize::{
    normalize_audio_codec, normalize_hdr, normalize_quality, normalize_video_codec,
};
use super::super::{FileType, MediaKind, Metadata};
use super::extractors::{
    apply_basic_extractors_and_collect_spans, extract_episode_and_collect_spans,
    extract_season_only_with_span, extract_subtitle_suffix_span,
    extract_subtitles_and_collect_spans,
};
use super::labels::{Label, NOISE_BRACKET_RE, classify_token};
use super::release_group::{
    is_metadata_fragment, is_noise_bracket, looks_like_release_group, trailing_release_group,
};
use super::title_resolver::{rebalance_title_boundaries, titles_from_cleaned_part};
use super::tokenizer::{Token, TokenKind, tokenize};

static TITLE_REPLACE_RE: LazyLock<Vec<(Regex, &'static str)>> = LazyLock::new(|| {
    vec![
        (Regex::new(r"[\.\[\]\{\}\(\)]").unwrap(), " "),
        (Regex::new(r"第[^.\[\]]+季").unwrap(), ""),
        (Regex::new(r"\s+-\s+").unwrap(), " "),
    ]
});
static TITLE_TRIM_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\s{2,}").unwrap());

static VIDEO_EXTENSIONS: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
    let extensions = [
        ".3g2", ".3gp", ".3gp2", ".asf", ".avi", ".divx", ".flv", ".iso", ".m4v", ".mk2", ".mk3d",
        ".mka", ".mkv", ".mov", ".mp4", ".mp4a", ".mpeg", ".mpg", ".ogg", ".ogm", ".ogv", ".qt",
        ".ra", ".ram", ".rm", ".ts", ".m2ts", ".vob", ".wav", ".webm", ".wma", ".wmv",
    ];
    HashSet::from(extensions)
});

static SUBTITLE_EXTENSIONS: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
    let extensions = [".srt", ".sub", ".idx", ".ass", ".ssa"];
    HashSet::from(extensions)
});

static LANG_DETECTOR: LazyLock<LanguageDetector> = LazyLock::new(|| {
    let languages = vec![Language::English, Language::Chinese, Language::Japanese];
    LanguageDetectorBuilder::from_languages(&languages).build()
});

static NAME_NORMALIZE_RE: LazyLock<Vec<Regex>> = LazyLock::new(|| {
    vec![
        Regex::new(r"[_（）《》]").unwrap(),
        Regex::new(r"[\[★](\S{1,4}年)?\S{1,2}月新番[\]★]").unwrap(),
        Regex::new(r"(?i)\[\d+(\.\d+)G?\]").unwrap(),
        Regex::new(r"(?i)10-?bit").unwrap(),
    ]
});

/// Token-pipeline entry point. Currently only handles a subset of fields.
/// Falls back to default values for anything not yet ported.
#[allow(dead_code)]
pub(crate) fn parse(name: &str) -> Box<Metadata> {
    let (body, file_type, extension) = normalize_name(name);
    let mut metadata = Metadata {
        file_type,
        extension,
        ..Default::default()
    };

    let mut tokens = tokenize(&body);
    let mut noise_spans: Vec<std::ops::Range<usize>> = Vec::new();
    label_tokens(&mut tokens);
    merge_split_codecs(&mut tokens);
    backfill_from_noise_brackets(&mut tokens, &mut metadata);

    extract_year(&mut tokens, &mut metadata);
    extract_resolution(&mut tokens, &mut metadata);
    extract_quality(&mut tokens, &mut metadata);
    extract_video_codec(&mut tokens, &mut metadata);
    extract_hdr(&mut tokens, &mut metadata);
    extract_frame_rate(&mut tokens, &mut metadata);

    // Fall back to the legacy body-regex extractors for fields the token
    // pipeline doesn't yet fully own (TMDB, multi-token audio codec, etc.).
    let legacy_spans = apply_basic_extractors_and_collect_spans(&body, &mut metadata);
    for span in legacy_spans {
        for token in tokens.iter_mut() {
            if spans_overlap(&token.span, &span) && token.label == Label::Unknown {
                token.label = Label::PromotionalNoise;
            }
        }
        noise_spans.push(span);
    }

    extract_episode_and_season(&body, &mut tokens, &mut noise_spans, &mut metadata);
    if metadata.file_type == FileType::Subtitle
        && let Some(span) = extract_subtitle_suffix_span(&body)
    {
        for token in tokens.iter_mut() {
            if span_fully_covers(&span, &token.span) {
                token.label = Label::SubtitleMarker;
            }
        }
        noise_spans.push(span);
    }
    extract_release_group(&body, &mut tokens, &mut metadata);
    extract_digit_only_episode(&body, &mut metadata);
    resolve_unknown_neighbors(&mut tokens);
    extract_titles(&body, &tokens, &noise_spans, &mut metadata);

    resolve_kind(&mut metadata);
    Box::new(metadata)
}

fn extract_digit_only_episode(body: &str, metadata: &mut Metadata) {
    if metadata.episode_number.is_some() {
        return;
    }
    let trimmed = body.trim();
    if trimmed.is_empty() {
        return;
    }
    if !trimmed.chars().all(|ch| ch.is_ascii_digit()) {
        return;
    }
    if let Ok(value) = trimmed.parse::<u32>() {
        metadata.episode_number = Some(value);
    }
}

fn extract_release_group(body: &str, tokens: &mut [Token], metadata: &mut Metadata) {
    // First pass: leading bracket. Accept a non-classifying first bracket as
    // release group if (a) its content qualifies, OR (b) the trailing body
    // starts with the promotional '★' marker (legacy heuristic).
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
        // Second pass: any later bracket that classifies (legacy fallback).
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

    if metadata.release_group.is_empty() {
        // Build a byte-level occupancy view from labeled tokens so the legacy
        // trailing-group helper still has the context it needs.
        let mut occupied = vec![false; body.len()];
        for token in tokens.iter() {
            if !matches!(token.label, Label::Unknown | Label::Title) {
                let span = token.span.clone();
                let end = span.end.min(occupied.len());
                let start = span.start.min(end);
                occupied[start..end].fill(true);
            }
        }
        if let Some((span, group)) =
            trailing_release_group(body, &occupied, is_metadata_fragment, is_noise_bracket)
        {
            metadata.release_group = group;
            for token in tokens.iter_mut() {
                if spans_overlap(&token.span, &span) {
                    token.label = Label::Group;
                }
            }
        }
    }
}

/// Implements the issue #81 "Unknown context judgement" rules in the title
/// resolver. Each Unknown token is re-classified using its labeled neighbors:
///
/// 1. Both neighbors non-Title non-Unknown → drop (PromotionalNoise).
/// 2. Token sits after a confirmed Episode and is pure-ASCII → drop
///    (defensive Source-Tag fallback even if the dictionary missed it).
/// 3. At least one neighbor is Title or Unknown → relabel as Title.
/// 4. Otherwise → drop.
// Issue #81 "Unknown context judgement". Currently applies a conservative
// subset: an Unknown token wedged between two noise neighbors is dropped
// only when its own text looks like a typical scene tag (short, ASCII, no
// spaces). Real title content — CJK characters, multi-word phrases, or
// longer ASCII strings — keeps its Unknown label and is later folded into
// the title candidate. Tightening the rule to the full issue 81 spec
// requires first labelling long/CJK Unknown tokens as Title, which is
// scheduled for a later slice.
fn resolve_unknown_neighbors(tokens: &mut [Token]) {
    let labels_snapshot: Vec<Label> = tokens.iter().map(|t| t.label).collect();
    let len = tokens.len();
    for (idx, token) in tokens.iter_mut().enumerate() {
        if token.label != Label::Unknown {
            continue;
        }
        if token.kind != TokenKind::Bare {
            // Bracketed/parenthesized Unknown content is treated as title-slot
            // material, not a scene tag, even if it looks short and ASCII.
            continue;
        }
        if !looks_like_scene_tag(token.text.as_str()) {
            continue;
        }
        let prev = (0..idx).rev().map(|i| labels_snapshot[i]).next();
        let next = ((idx + 1)..len).map(|i| labels_snapshot[i]).next();
        let prev_is_noise = prev
            .map(|l| !matches!(l, Label::Title | Label::Unknown))
            .unwrap_or(false);
        let next_is_noise = next
            .map(|l| !matches!(l, Label::Title | Label::Unknown))
            .unwrap_or(false);
        if prev_is_noise && next_is_noise {
            token.label = Label::SourceTag;
        }
    }
}

fn looks_like_scene_tag(text: &str) -> bool {
    if text.is_empty() || text.len() > 5 {
        return false;
    }
    // Scene tags are conventionally ALL-CAPS short ASCII (e.g. DSNP, NF,
    // AMZN). Mixed-case ASCII words like "Mashle" are title content.
    text.chars()
        .all(|ch| ch.is_ascii_uppercase() || ch.is_ascii_digit() || ch == '-')
        && text.chars().any(|ch| ch.is_ascii_uppercase())
}

fn extract_titles(
    body: &str,
    tokens: &[Token],
    noise_spans: &[std::ops::Range<usize>],
    metadata: &mut Metadata,
) {
    let title_byte: Vec<bool> = title_byte_mask(body, tokens, noise_spans);
    let mut candidate = String::with_capacity(body.len());
    for (idx, ch) in body.char_indices() {
        if title_byte[idx] {
            candidate.push(ch);
        } else {
            candidate.push(' ');
        }
    }
    let mut text = candidate.trim().to_owned();
    if text.is_empty() {
        return;
    }
    // Pure-digit candidate is the "8.mp4" case — already handled as episode
    // number, never a title.
    if text.chars().all(|ch| ch.is_ascii_digit()) {
        return;
    }
    // Mixed-content candidate must contain at least one letter, CJK char, or
    // digit between separators (rejects "..." / "---" residues).
    if !text
        .chars()
        .any(|ch| ch.is_alphanumeric() || ('\u{4e00}'..='\u{9fff}').contains(&ch))
    {
        return;
    }
    for (re, replace_to) in TITLE_REPLACE_RE.iter() {
        text = re.replace_all(text.as_str(), *replace_to).into_owned();
    }

    let mut titles = Vec::new();
    for part in text.split('/') {
        let cleaned = part
            .trim_matches(|ch: char| matches!(ch, '.' | '-' | '_' | ' '))
            .trim()
            .to_owned();
        let cleaned = TITLE_TRIM_RE.replace_all(&cleaned, " ").trim().to_owned();
        if cleaned.is_empty() {
            continue;
        }
        titles.extend(titles_from_cleaned_part(cleaned.as_str(), &LANG_DETECTOR));
    }
    rebalance_title_boundaries(&mut titles);
    if !titles.is_empty() {
        metadata.titles = titles;
    }
}

fn title_byte_mask(
    body: &str,
    tokens: &[Token],
    noise_spans: &[std::ops::Range<usize>],
) -> Vec<bool> {
    let mut mask = vec![true; body.len()];
    for token in tokens.iter() {
        if matches!(token.label, Label::Unknown | Label::Title) {
            continue;
        }
        let end = token.span.end.min(mask.len());
        let start = token.span.start.min(end);
        mask[start..end].fill(false);
    }
    for span in noise_spans.iter() {
        let end = span.end.min(mask.len());
        let start = span.start.min(end);
        mask[start..end].fill(false);
    }
    if let Some(boundary) = standard_scene_episode_boundary(body, tokens) {
        for byte in mask.iter_mut().skip(boundary) {
            *byte = false;
        }
    }
    mask
}

/// Mirrors the legacy `standard_scene_episode_boundary`: when the body has a
/// scene-style `S\d+E\d+` marker and the preceding bytes contain a title-like
/// prefix, anything past the marker is the scene episode's own title and
/// should not be folded into the main title.
fn standard_scene_episode_boundary(body: &str, tokens: &[Token]) -> Option<usize> {
    static COMPACT_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)S(?P<season>\d{1,2})\s*[-._ ]?E(?P<episode>\d{1,4})(?:\s*[-~]\s*\d{1,4})?")
            .unwrap()
    });
    if !tokens.iter().any(|t| t.label == Label::Episode) {
        return None;
    }
    let caps = COMPACT_RE.captures(body)?;
    let span = caps.get(0)?.start();
    let prefix = body[..span]
        .trim_matches(|ch: char| matches!(ch, '.' | '-' | '_' | ' ' | '[' | ']' | '(' | ')'));
    if prefix.is_empty() || prefix.contains('/') {
        return None;
    }
    if !prefix
        .chars()
        .any(|ch| ch.is_alphabetic() || ('\u{4e00}'..='\u{9fff}').contains(&ch))
    {
        return None;
    }
    Some(span)
}

fn label_tokens(tokens: &mut [Token]) {
    for token in tokens.iter_mut() {
        if token.label != Label::Unknown {
            continue;
        }
        let label = classify_token(token.text.as_str());
        if label != Label::Unknown {
            token.label = label;
            continue;
        }
        if matches!(token.kind, TokenKind::Bracketed | TokenKind::Parenthesized)
            && bracket_content_is_pure_noise(token.text.as_str())
        {
            token.label = Label::PromotionalNoise;
        }
    }
}

fn bracket_content_is_pure_noise(content: &str) -> bool {
    let trimmed = content.trim();
    if trimmed.is_empty() {
        return true;
    }
    if NOISE_BRACKET_RE.is_match(trimmed) {
        return true;
    }
    if is_noise_bracket(trimmed) {
        return true;
    }
    if is_metadata_fragment(trimmed) {
        return true;
    }
    let pieces: Vec<&str> = trimmed.split_whitespace().collect();
    if pieces.len() <= 1 {
        return false;
    }
    pieces.iter().all(|piece| is_noise_label(classify_token(piece)))
}

fn is_noise_label(label: Label) -> bool {
    matches!(
        label,
        Label::VideoCodec
            | Label::AudioCodec
            | Label::Quality
            | Label::Hdr
            | Label::Resolution
            | Label::FrameRate
            | Label::SourceTag
            | Label::SubtitleMarker
            | Label::PromotionalNoise
    )
}

/// For bracket/paren tokens already marked as PromotionalNoise, extract the
/// metadata fields encoded in their multi-word content so downstream
/// extractors can still pick up codec/resolution/quality from inside noise.
fn backfill_from_noise_brackets(tokens: &mut [Token], metadata: &mut Metadata) {
    for token in tokens.iter() {
        if token.label != Label::PromotionalNoise {
            continue;
        }
        if !matches!(token.kind, TokenKind::Bracketed | TokenKind::Parenthesized) {
            continue;
        }
        for piece in token
            .text
            .split(|ch: char| ch.is_whitespace() || matches!(ch, '@' | '/' | '_'))
        {
            let piece = piece.trim_matches(|ch: char| !ch.is_alphanumeric() && ch != '+');
            if piece.is_empty() {
                continue;
            }
            match classify_token(piece) {
                Label::Resolution if metadata.resolution.is_empty() => {
                    let lower = piece.to_ascii_lowercase();
                    metadata.resolution = if lower == "4k" {
                        "2160p".to_owned()
                    } else if let Some((_, height)) = lower.split_once('x') {
                        format!("{height}p")
                    } else {
                        lower
                    };
                }
                Label::Quality if metadata.quality.is_empty() => {
                    metadata.quality = normalize_quality(piece);
                }
                Label::VideoCodec if metadata.video_codec.is_empty() => {
                    metadata.video_codec = normalize_video_codec(piece);
                }
                Label::AudioCodec if metadata.audio_codec.is_empty() => {
                    metadata.audio_codec = normalize_audio_codec(piece);
                }
                Label::Hdr if metadata.hdr.is_empty() => {
                    let normalized = normalize_hdr(piece);
                    if normalized == "DV" || metadata.hdr.is_empty() {
                        metadata.hdr = normalized;
                    }
                }
                Label::FrameRate if metadata.frame_rate.is_empty() => {
                    metadata.frame_rate = piece.to_lowercase();
                }
                _ => {}
            }
        }
    }
}

/// Re-merge codec patterns that the dot-aggressive tokenizer split apart, e.g.
/// `H.264` → tokens `H` + `264` → relabel as one `VideoCodec`.
fn merge_split_codecs(tokens: &mut [Token]) {
    for i in 0..tokens.len().saturating_sub(1) {
        if tokens[i].kind != TokenKind::Bare || tokens[i + 1].kind != TokenKind::Bare {
            continue;
        }
        let left = tokens[i].text.to_uppercase();
        let right = &tokens[i + 1].text;
        if (left == "H" || left == "X") && (right == "264" || right == "265") {
            tokens[i].label = Label::VideoCodec;
            tokens[i + 1].label = Label::VideoCodec;
        }
        if left == "DOLBY" && right.eq_ignore_ascii_case("Vision") {
            tokens[i].label = Label::Hdr;
            tokens[i + 1].label = Label::Hdr;
        }
        // "Ultra HD" — promotional, drops out of title candidate.
        if left == "ULTRA" && right.eq_ignore_ascii_case("HD") {
            tokens[i].label = Label::PromotionalNoise;
            tokens[i + 1].label = Label::PromotionalNoise;
        }
        // "DoVi P7" — Dolby Vision profile, noise.
        if (left == "DOVI" || left == "DV")
            && right
                .chars()
                .next()
                .map(|c| c == 'P' || c == 'p')
                .unwrap_or(false)
            && right[1..].chars().all(|c| c.is_ascii_digit())
        {
            tokens[i + 1].label = Label::PromotionalNoise;
        }
    }
}

fn extract_year(tokens: &mut [Token], metadata: &mut Metadata) {
    if !metadata.year.is_empty() {
        return;
    }
    let mut chosen_idx: Option<usize> = None;
    for (idx, token) in tokens.iter().enumerate() {
        if token.label == Label::Year {
            chosen_idx = Some(idx);
        }
    }
    let Some(chosen) = chosen_idx else {
        return;
    };
    metadata.year = tokens[chosen].text.clone();
    // When multiple Year-labeled tokens are present (e.g. "Reply 1988 (2015)"),
    // only the last one is the actual year; earlier candidates are title
    // content and should drop their Year label so they re-enter the title.
    for (idx, token) in tokens.iter_mut().enumerate() {
        if idx != chosen && token.label == Label::Year {
            token.label = Label::Unknown;
        }
    }
}

fn extract_resolution(tokens: &mut [Token], metadata: &mut Metadata) {
    if !metadata.resolution.is_empty() {
        return;
    }
    let mut chosen = None;
    for token in tokens.iter() {
        if token.label != Label::Resolution {
            continue;
        }
        let lower = token.text.to_ascii_lowercase();
        chosen = Some(if lower == "4k" {
            "2160p".to_owned()
        } else if let Some((_, height)) = lower.split_once('x') {
            format!("{height}p")
        } else {
            lower
        });
    }
    if let Some(resolution) = chosen {
        metadata.resolution = resolution;
    }
}

fn extract_quality(tokens: &mut [Token], metadata: &mut Metadata) {
    if !metadata.quality.is_empty() {
        return;
    }
    let mut chosen = None;
    for token in tokens.iter() {
        if token.label == Label::Quality {
            chosen = Some(normalize_quality(token.text.as_str()));
        }
    }
    if let Some(quality) = chosen {
        metadata.quality = quality;
    }
}

fn extract_video_codec(tokens: &mut [Token], metadata: &mut Metadata) {
    if !metadata.video_codec.is_empty() {
        return;
    }
    let mut i = 0;
    let mut chosen = None;
    while i < tokens.len() {
        let token = &tokens[i];
        if token.label != Label::VideoCodec {
            i += 1;
            continue;
        }
        let mut text = token.text.clone();
        if (text.eq_ignore_ascii_case("H") || text.eq_ignore_ascii_case("X"))
            && i + 1 < tokens.len()
            && tokens[i + 1].label == Label::VideoCodec
            && matches!(tokens[i + 1].text.as_str(), "264" | "265")
        {
            text.push_str(tokens[i + 1].text.as_str());
            i += 1;
        }
        chosen = Some(normalize_video_codec(text.as_str()));
        i += 1;
    }
    if let Some(codec) = chosen {
        metadata.video_codec = codec;
    }
}

fn extract_hdr(tokens: &mut [Token], metadata: &mut Metadata) {
    if !metadata.hdr.is_empty() {
        return;
    }
    let mut selected = None;
    let mut fallback = None;
    for token in tokens.iter() {
        if token.label != Label::Hdr {
            continue;
        }
        let normalized = normalize_hdr(token.text.as_str());
        if normalized == "DV" {
            selected = Some(normalized);
        } else {
            fallback = Some(normalized);
        }
    }
    if let Some(value) = selected.or(fallback) {
        metadata.hdr = value;
    }
}

fn extract_frame_rate(tokens: &mut [Token], metadata: &mut Metadata) {
    if !metadata.frame_rate.is_empty() {
        return;
    }
    let mut chosen = None;
    for token in tokens.iter() {
        if token.label == Label::FrameRate {
            chosen = Some(token.text.to_lowercase());
        }
    }
    if let Some(value) = chosen {
        metadata.frame_rate = value;
    }
}

fn extract_episode_and_season(
    body: &str,
    tokens: &mut [Token],
    noise_spans: &mut Vec<std::ops::Range<usize>>,
    metadata: &mut Metadata,
) {
    if let Some(extraction) = extract_episode_and_collect_spans(body) {
        metadata.season_number = extraction.season_number;
        metadata.episode_number = Some(extraction.episode_number);
        metadata.second_episode_number = extraction.second_episode_number;
        for span in extraction.spans {
            for token in tokens.iter_mut() {
                if span_fully_covers(&span, &token.span) {
                    token.label = Label::Episode;
                }
            }
            noise_spans.push(span);
        }
    }
    if metadata.season_number.is_none()
        && let Some((span, season_number)) = extract_season_only_with_span(body)
    {
        metadata.season_number = Some(season_number);
        for token in tokens.iter_mut() {
            if span_fully_covers(&span, &token.span) {
                token.label = Label::Episode;
            }
        }
        noise_spans.push(span);
    }

    let subtitle_extraction = extract_subtitles_and_collect_spans(body);
    if !subtitle_extraction.languages.is_empty() {
        metadata.subtitles = subtitle_extraction.languages;
        for span in subtitle_extraction.spans {
            for token in tokens.iter_mut() {
                if span_fully_covers(&span, &token.span) {
                    token.label = Label::SubtitleMarker;
                }
            }
            noise_spans.push(span);
        }
    }
}

fn span_fully_covers(outer: &std::ops::Range<usize>, inner: &std::ops::Range<usize>) -> bool {
    outer.start <= inner.start && outer.end >= inner.end
}

fn spans_overlap(a: &std::ops::Range<usize>, b: &std::ops::Range<usize>) -> bool {
    a.start < b.end && b.start < a.end
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

fn resolve_kind(metadata: &mut Metadata) {
    metadata.media_kind = if metadata.episode_number.is_some() {
        MediaKind::TvEpisode
    } else if metadata.file_type != FileType::Unknown || !metadata.titles.is_empty() {
        MediaKind::Movie
    } else {
        MediaKind::Unknown
    };
}

fn normalize_name(name: &str) -> (String, FileType, String) {
    let (body, extension) = split_extension(name);
    let mut normalized = body.replace("【", "[");
    normalized = normalized.replace("】", "]");
    normalized = normalized.replace("精校", ".");
    for re in NAME_NORMALIZE_RE.iter() {
        normalized = re.replace_all(&normalized, ".").into_owned();
    }
    let normalized = normalized.trim().to_owned();

    let file_type = if VIDEO_EXTENSIONS.contains(extension.as_str()) {
        FileType::Video
    } else if SUBTITLE_EXTENSIONS.contains(extension.as_str()) {
        FileType::Subtitle
    } else {
        FileType::Unknown
    };

    (normalized, file_type, extension)
}

fn split_extension(name: &str) -> (String, String) {
    if let Some(dot_index) = name.rfind('.') {
        let extension = name[dot_index..].trim().to_lowercase();
        if VIDEO_EXTENSIONS.contains(extension.as_str())
            || SUBTITLE_EXTENSIONS.contains(extension.as_str())
        {
            return (name[..dot_index].to_owned(), extension);
        }
    }
    (name.to_owned(), String::new())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_simple_movie_metadata_via_token_pipeline() {
        let meta = parse("Movie.2024.1080p.WEB-DL.H.264.AAC.mkv");

        assert_eq!(meta.year, "2024");
        assert_eq!(meta.resolution, "1080p");
        assert_eq!(meta.quality, "WEB-DL");
        assert_eq!(meta.video_codec, "H264");
        assert_eq!(meta.audio_codec, "AAC");
        assert_eq!(meta.extension, ".mkv");
        assert_eq!(meta.file_type, FileType::Video);
    }

    #[test]
    fn extracts_4k_resolution_and_dv_hdr() {
        let meta = parse("Movie.2024.4K.HDR10+.Dolby.Vision.WEB-DL");

        assert_eq!(meta.resolution, "2160p");
        // Dolby Vision should win over HDR10+
        assert_eq!(meta.hdr, "DV");
    }

    #[test]
    fn extracts_frame_rate_token() {
        let meta = parse("Show.S01E02.1080p.60fps.WEB-DL");

        assert_eq!(meta.frame_rate, "60fps");
        assert_eq!(meta.resolution, "1080p");
    }
}
