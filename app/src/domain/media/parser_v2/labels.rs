#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Label {
    VideoCodec,
    AudioCodec,
    Quality,
    Hdr,
    SourceTag,
    SubtitleMarker,
}

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
}
