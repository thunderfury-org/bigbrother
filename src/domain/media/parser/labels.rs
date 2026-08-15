use std::sync::LazyLock;

use regex::Regex;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum Label {
    #[default]
    Unknown,
    // Title and TmdbId are part of the issue-81 Label vocabulary but are not
    // produced by any current extractor. Title tokens are implied (Unknown
    // tokens that survive title resolution become title content); TmdbId is
    // extracted from the body without going through token labels. They are
    // kept as enum variants so the type matches the PRD and future extractors
    // can populate them.
    #[allow(dead_code)]
    Title,
    Episode,
    Year,
    #[allow(dead_code)]
    TmdbId,
    Resolution,
    Quality,
    Hdr,
    FrameRate,
    VideoCodec,
    AudioCodec,
    SourceTag,
    Group,
    SubtitleMarker,
    PromotionalNoise,
}

// === Literal keyword vocabularies (used by `classify_bare`) ===

pub(crate) const VIDEO_CODEC_KEYWORDS: &[&str] =
    &["H264", "H265", "X264", "X265", "HEVC", "AVC", "AV1", "VP9"];

pub(crate) const AUDIO_CODEC_KEYWORDS: &[&str] = &[
    "AAC", "FLAC", "DTS", "DTS-HD", "TRUEHD", "ATMOS", "EAC3", "AC3", "DDP", "DD",
];

pub(crate) const QUALITY_KEYWORDS: &[&str] = &[
    "WEB-DL", "WEBDL", "WEBRIP", "BLURAY", "REMUX", "BDRIP", "BRRIP",
];

pub(crate) const HDR_KEYWORDS: &[&str] = &["HDR10+", "HDR10", "HDR", "DV", "DOVI", "HLG"];

pub(crate) const SOURCE_TAG_KEYWORDS: &[&str] = &[
    "DSNP", "AMZN", "ATVP", "NF", "DSNY", "HMAX", "BAHA", "B-GLOBAL", "VIUTV",
];

pub(crate) const SUBTITLE_MARKER_KEYWORDS: &[&str] = &[
    "CHS", "CHT", "BIG5", "ZH-HANS", "ZH-HANT", "JP", "简中", "繁中", "簡中", "簡體", "繁體",
    "简体", "繁体",
];

// === Regex-friendly alternatives (literal forms + minor variant patterns) ===
// Each slice is joined with `|` when building composite regexes. Entries may
// contain regex metacharacters (e.g. optional hyphen `WEB-?DL`).

pub(crate) const QUALITY_PATTERNS: &[&str] = &[
    r"WEB-?DL",
    r"WEB-?RIP",
    r"WEBRIP",
    r"BLU-?RAY",
    r"REMUX",
    r"BD-?RIP",
    r"BR-?RIP",
    r"WEB",
];

pub(crate) const VIDEO_CODEC_PATTERNS: &[&str] =
    &[r"HEVC", r"AVC", r"AV1", r"VP-9", r"H\.?26[45]", r"X26[45]"];

pub(crate) const AUDIO_CODEC_PATTERNS: &[&str] =
    &[r"AAC", r"FLAC", r"DTS(?:-?HD)?", r"TRUEHD", r"ATMOS"];

pub(crate) const HDR_PATTERNS: &[&str] = &[r"HDR10\+?", r"HDR", r"DV", r"DOVI", r"HLG"];

pub(crate) const FRAME_RATE_PATTERNS: &[&str] = &[r"\d{2,3}FPS"];

pub(crate) const RESOLUTION_PATTERNS: &[&str] = &[r"4K", r"\d{3,4}P", r"\d{3,4}X\d{3,4}"];

pub(crate) const FILE_SIZE_PATTERNS: &[&str] = &[r"\d+(?:\.\d+)?G(?:B)?"];

pub(crate) const CRC_HASH_PATTERNS: &[&str] = &[r"[A-F0-9]{8}"];

pub(crate) const CONTAINER_EXT_KEYWORDS: &[&str] = &["MKV", "MP4", "SRT", "ASS", "SSA", "PGS"];

pub(crate) const SUBTITLE_MARKER_PATTERNS: &[&str] = &[
    r"CHS(?:[._-]?JP)?",
    r"CHT",
    r"BIG5",
    r"ZH-HANS",
    r"ZH-HANT",
    r"JP",
    r"简中",
    r"繁中",
    r"簡中",
    r"簡體",
    r"繁體",
    r"简体",
    r"繁体",
];

pub(crate) const SUBTITLE_NOISE_FRAGMENTS: &[&str] = &["字幕", "内封", "外挂字幕"];

// Multi-language subtitle fragments that should be flagged as substring noise
// (TECHNICAL_FRAGMENT_RE) but are NOT, on their own, a full bracket noise
// match — those phrases need to combine with a container marker first (see
// `BRACKET_SUBTITLE_PHRASES` such as `简繁日多语MKV`).
pub(crate) const SUBTITLE_MULTILANG_FRAGMENTS: &[&str] = &["简繁日多语"];

pub(crate) const QUALITY_MISC_PATTERNS: &[&str] =
    &[r"UHD", r"Ultra\s+HD", r"SDR", r"EXTENDED", r"P\d+", r"HQ"];

pub(crate) const STREAMING_PROVIDER_KEYWORDS: &[&str] = &["Baha", "B-Global", "ViuTV"];

pub(crate) const EPISODE_TYPE_KEYWORDS: &[&str] = &["OVA", "OAD", "NCOP", "NCED", "Fin"];

pub(crate) const PROMOTIONAL_NOISE_KEYWORDS: &[&str] =
    &["附外挂字幕", "招募翻译校对", "日語原聲", "日文自動產生字幕"];

pub(crate) const TECHNICAL_FRAGMENT_EXTRAS: &[&str] =
    &["进化版", "Web先行版", "先行版", "英语中字", "蓝光原盘"];

pub(crate) const BRACKET_SUBTITLE_PHRASES: &[&str] = &[
    "简繁内封",
    "简繁日内封字幕",
    "简日双语MP4/繁日双语MP4/简繁日多语MKV",
];

// Tokens that count as "purely technical noise" when they form the whole of a
// release-group candidate (e.g. a trailing `-WEB-DL`).
pub(crate) const TECHNICAL_GROUP_PATTERNS: &[&str] = &[
    r"WEB",
    r"DL",
    r"DV",
    r"HDR",
    r"HDR10",
    r"HDR10\+",
    r"AAC",
    r"DD",
    r"DDP",
    r"DTS",
    r"DTS-HD",
    r"HD",
    r"MA",
    r"TRUEHD",
    r"ATMOS",
    r"HEVC",
    r"AVC",
    r"H264",
    r"H265",
    r"REMUX",
    r"BLURAY",
    r"HQ",
    r"\d{2,3}FPS",
    r"\d+",
];

// === Composite regexes built from the slices above ===

pub(crate) static TECHNICAL_FRAGMENT_RE: LazyLock<Regex> = LazyLock::new(|| {
    let alternatives = collect_alternatives(&[
        QUALITY_PATTERNS,
        VIDEO_CODEC_PATTERNS,
        AUDIO_CODEC_PATTERNS,
        HDR_PATTERNS,
        FRAME_RATE_PATTERNS,
        RESOLUTION_PATTERNS,
        FILE_SIZE_PATTERNS,
        CRC_HASH_PATTERNS,
        CONTAINER_EXT_KEYWORDS,
        SUBTITLE_MARKER_PATTERNS,
        SUBTITLE_MULTILANG_FRAGMENTS,
        SUBTITLE_NOISE_FRAGMENTS,
        QUALITY_MISC_PATTERNS,
        STREAMING_PROVIDER_KEYWORDS,
        EPISODE_TYPE_KEYWORDS,
        PROMOTIONAL_NOISE_KEYWORDS,
        TECHNICAL_FRAGMENT_EXTRAS,
    ]);
    Regex::new(&format!("(?i)(?:{alternatives})")).unwrap()
});

pub(crate) static NOISE_BRACKET_RE: LazyLock<Regex> = LazyLock::new(|| {
    let alternatives = collect_alternatives(&[
        QUALITY_PATTERNS,
        VIDEO_CODEC_PATTERNS,
        AUDIO_CODEC_PATTERNS,
        HDR_PATTERNS,
        FRAME_RATE_PATTERNS,
        RESOLUTION_PATTERNS,
        FILE_SIZE_PATTERNS,
        CRC_HASH_PATTERNS,
        CONTAINER_EXT_KEYWORDS,
        SUBTITLE_MARKER_PATTERNS,
        BRACKET_SUBTITLE_PHRASES,
        SUBTITLE_NOISE_FRAGMENTS,
        PROMOTIONAL_NOISE_KEYWORDS,
        EPISODE_TYPE_KEYWORDS,
        QUALITY_MISC_PATTERNS,
        STREAMING_PROVIDER_KEYWORDS,
    ]);
    Regex::new(&format!("(?i)^(?:{alternatives})$")).unwrap()
});

pub(crate) static TECHNICAL_GROUP_RE: LazyLock<Regex> = LazyLock::new(|| {
    let alternatives = collect_alternatives(&[TECHNICAL_GROUP_PATTERNS]);
    Regex::new(&format!("(?i)^(?:{alternatives})$")).unwrap()
});

// === Single-token pattern matchers (used by `classify_token`) ===

static YEAR_TOKEN_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(?:19\d{2}|20\d{2})$").unwrap());

static RESOLUTION_TOKEN_RE: LazyLock<Regex> = LazyLock::new(|| {
    let alternatives = collect_alternatives(&[RESOLUTION_PATTERNS]);
    Regex::new(&format!("(?i)^(?:{alternatives})$")).unwrap()
});

static FRAME_RATE_TOKEN_RE: LazyLock<Regex> = LazyLock::new(|| {
    let alternatives = collect_alternatives(&[FRAME_RATE_PATTERNS]);
    Regex::new(&format!("(?i)^(?:{alternatives})$")).unwrap()
});

static FILE_SIZE_TOKEN_RE: LazyLock<Regex> = LazyLock::new(|| {
    let alternatives = collect_alternatives(&[FILE_SIZE_PATTERNS]);
    Regex::new(&format!("(?i)^(?:{alternatives})$")).unwrap()
});

static CRC_HASH_TOKEN_RE: LazyLock<Regex> = LazyLock::new(|| {
    let alternatives = collect_alternatives(&[CRC_HASH_PATTERNS]);
    Regex::new(&format!("(?i)^(?:{alternatives})$")).unwrap()
});

static SEASON_EPISODE_TOKEN_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)^S\d{1,2}E\d{1,4}(?:-E?\d{1,4})?$").unwrap());

static SEASON_TOKEN_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)^S\d{1,2}$").unwrap());

static QUALITY_MISC_TOKEN_RE: LazyLock<Regex> = LazyLock::new(|| {
    let alternatives = collect_alternatives(&[QUALITY_MISC_PATTERNS]);
    Regex::new(&format!("(?i)^(?:{alternatives})$")).unwrap()
});

/// Classify a single bare token by its text. Combines literal-keyword
/// classification (`classify_bare`) with pattern-based labels (`Resolution`,
/// `FrameRate`, `Year`, `Episode`). Returns `Label::Unknown` if no rule fires.
pub(crate) fn classify_token(text: &str) -> Label {
    if let Some(label) = classify_bare(text) {
        return label;
    }
    if SEASON_EPISODE_TOKEN_RE.is_match(text) {
        return Label::Episode;
    }
    if SEASON_TOKEN_RE.is_match(text) {
        return Label::Episode;
    }
    if YEAR_TOKEN_RE.is_match(text) {
        return Label::Year;
    }
    if RESOLUTION_TOKEN_RE.is_match(text) {
        return Label::Resolution;
    }
    if FRAME_RATE_TOKEN_RE.is_match(text) {
        return Label::FrameRate;
    }
    if FILE_SIZE_TOKEN_RE.is_match(text) || CRC_HASH_TOKEN_RE.is_match(text) {
        return Label::PromotionalNoise;
    }
    if QUALITY_MISC_TOKEN_RE.is_match(text) {
        return Label::PromotionalNoise;
    }
    if CONTAINER_EXT_KEYWORDS
        .iter()
        .any(|k| k.eq_ignore_ascii_case(text))
    {
        return Label::PromotionalNoise;
    }
    if PROMOTIONAL_NOISE_KEYWORDS
        .iter()
        .chain(TECHNICAL_FRAGMENT_EXTRAS.iter())
        .chain(EPISODE_TYPE_KEYWORDS.iter())
        .chain(STREAMING_PROVIDER_KEYWORDS.iter())
        .any(|k| *k == text || k.eq_ignore_ascii_case(text))
    {
        return Label::PromotionalNoise;
    }
    Label::Unknown
}

fn collect_alternatives(slices: &[&[&str]]) -> String {
    slices
        .iter()
        .flat_map(|slice| slice.iter().copied())
        .collect::<Vec<_>>()
        .join("|")
}

pub(crate) fn classify_bare(token: &str) -> Option<Label> {
    let upper = token.to_ascii_uppercase();
    let upper = upper.as_str();

    if VIDEO_CODEC_KEYWORDS
        .iter()
        .any(|k| k.eq_ignore_ascii_case(upper))
    {
        return Some(Label::VideoCodec);
    }
    if AUDIO_CODEC_KEYWORDS
        .iter()
        .any(|k| k.eq_ignore_ascii_case(upper))
    {
        return Some(Label::AudioCodec);
    }
    if QUALITY_KEYWORDS
        .iter()
        .any(|k| k.eq_ignore_ascii_case(upper))
    {
        return Some(Label::Quality);
    }
    if HDR_KEYWORDS.iter().any(|k| k.eq_ignore_ascii_case(upper)) {
        return Some(Label::Hdr);
    }
    if SOURCE_TAG_KEYWORDS
        .iter()
        .any(|k| k.eq_ignore_ascii_case(upper))
    {
        return Some(Label::SourceTag);
    }
    if SUBTITLE_MARKER_KEYWORDS
        .iter()
        .any(|k| *k == token || k.eq_ignore_ascii_case(upper))
    {
        return Some(Label::SubtitleMarker);
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    // Cycle 1 & 2
    #[test]
    fn classifies_h264_as_video_codec() {
        assert_eq!(classify_bare("H264"), Some(Label::VideoCodec));
    }

    #[test]
    fn classifies_aac_as_audio_codec() {
        assert_eq!(classify_bare("AAC"), Some(Label::AudioCodec));
    }

    // Cycle 3: Quality
    #[test]
    fn classifies_quality_keywords() {
        for kw in &["WEB-DL", "BluRay", "Remux", "WEBRip", "BDRip", "BRRip"] {
            assert_eq!(classify_bare(kw), Some(Label::Quality), "failed for {kw}");
        }
    }

    // Cycle 4: HDR
    #[test]
    fn classifies_hdr_keywords() {
        for kw in &["HDR10+", "HDR10", "HDR", "DV", "DoVi", "HLG"] {
            assert_eq!(classify_bare(kw), Some(Label::Hdr), "failed for {kw}");
        }
    }

    // Cycle 5: VideoCodec complete
    #[test]
    fn classifies_video_codec_keywords() {
        for kw in &["H265", "HEVC", "AVC", "AV1", "VP9", "X264", "X265"] {
            assert_eq!(
                classify_bare(kw),
                Some(Label::VideoCodec),
                "failed for {kw}"
            );
        }
    }

    // Cycle 6: AudioCodec complete
    #[test]
    fn classifies_audio_codec_keywords() {
        for kw in &[
            "FLAC", "DTS", "DTS-HD", "TrueHD", "Atmos", "EAC3", "AC3", "DDP", "DD",
        ] {
            assert_eq!(
                classify_bare(kw),
                Some(Label::AudioCodec),
                "failed for {kw}"
            );
        }
    }

    // Cycle 7: SourceTag (CONTEXT.md Source Tag)
    #[test]
    fn classifies_source_tags() {
        for kw in &[
            "DSNP", "AMZN", "ATVP", "NF", "DSNY", "HMAX", "Baha", "B-Global", "ViuTV",
        ] {
            assert_eq!(classify_bare(kw), Some(Label::SourceTag), "failed for {kw}");
        }
    }

    // Cycle 8: SubtitleMarker
    #[test]
    fn classifies_subtitle_markers() {
        for kw in &[
            "CHS", "CHT", "BIG5", "ZH-HANS", "ZH-HANT", "JP", "简中", "繁中",
        ] {
            assert_eq!(
                classify_bare(kw),
                Some(Label::SubtitleMarker),
                "failed for {kw}"
            );
        }
    }

    // Cycle 9: case-insensitive
    #[test]
    fn classify_bare_is_case_insensitive() {
        assert_eq!(classify_bare("h264"), Some(Label::VideoCodec));
        assert_eq!(classify_bare("web-dl"), Some(Label::Quality));
        assert_eq!(classify_bare("dsnp"), Some(Label::SourceTag));
    }

    // Cycle 10: unknown → None
    #[test]
    fn unknown_token_returns_none() {
        assert_eq!(classify_bare("Perfect"), None);
        assert_eq!(classify_bare("Crown"), None);
    }

    #[test]
    fn technical_fragment_regex_matches_common_noise() {
        for kw in &[
            "1080p",
            "WEB-DL",
            "H.264",
            "HEVC",
            "HDR10+",
            "DTS-HD",
            "60fps",
            "1920x1080",
            "Baha",
            "OVA",
            "蓝光原盘",
        ] {
            assert!(
                TECHNICAL_FRAGMENT_RE.is_match(kw),
                "TECHNICAL_FRAGMENT_RE failed to match {kw}"
            );
        }
    }

    #[test]
    fn noise_bracket_regex_matches_anchored_noise() {
        for kw in &["1080p", "WEB-DL", "HDR", "简繁内封", "60fps"] {
            assert!(
                NOISE_BRACKET_RE.is_match(kw),
                "NOISE_BRACKET_RE failed to match {kw}"
            );
        }
        assert!(!NOISE_BRACKET_RE.is_match("Perfect Crown"));
    }

    #[test]
    fn technical_group_regex_matches_pure_noise_groups() {
        for kw in &["WEB", "DL", "H264", "REMUX", "60FPS", "5"] {
            assert!(
                TECHNICAL_GROUP_RE.is_match(kw),
                "TECHNICAL_GROUP_RE failed to match {kw}"
            );
        }
        assert!(!TECHNICAL_GROUP_RE.is_match("BTN"));
    }

    #[test]
    fn classify_token_recognizes_year_resolution_and_frame_rate() {
        assert_eq!(classify_token("2024"), Label::Year);
        assert_eq!(classify_token("1080p"), Label::Resolution);
        assert_eq!(classify_token("2160P"), Label::Resolution);
        assert_eq!(classify_token("4K"), Label::Resolution);
        assert_eq!(classify_token("60fps"), Label::FrameRate);
    }

    #[test]
    fn classify_token_recognizes_scene_episode_marker() {
        assert_eq!(classify_token("S01E02"), Label::Episode);
        assert_eq!(classify_token("S1E12-E13"), Label::Episode);
    }

    #[test]
    fn classify_token_falls_back_to_unknown() {
        assert_eq!(classify_token("Perfect"), Label::Unknown);
        assert_eq!(classify_token("BTN"), Label::Unknown);
    }
}
