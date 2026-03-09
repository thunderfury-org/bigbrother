use std::sync::LazyLock;

use lingua::Language;
use regex::Regex;

static AUDIO_NORMALIZE_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\d\.\d").unwrap());

/// Normalizes a language to a standard format
pub(super) fn normalize_language(l: Language) -> String {
    match l {
        Language::Chinese => super::LANGUAGE_CHINESE.to_owned(),
        Language::Japanese => super::LANGUAGE_JAPANESE.to_owned(),
        Language::English => super::LANGUAGE_ENGLISH.to_owned(),
    }
}

/// Normalizes quality string to a standard format
pub(super) fn normalize_quality(quality: &str) -> String {
    let quality = quality.to_lowercase().replace(".", "");
    if quality.contains("remux") {
        return "Remux".to_owned();
    }

    match quality.as_str() {
        "web-dl" | "webdl" => "WEB-DL".to_owned(),
        "web-rip" | "webrip" => "WEBRip".to_owned(),
        "bluray" | "blu-ray" => "BluRay".to_owned(),
        "bdrip" | "bd-rip" => "BDRip".to_owned(),
        "brrip" | "br-rip" => "BRRip".to_owned(),
        _ => quality,
    }
}

/// Normalizes video codec string to a standard format
pub(super) fn normalize_video_codec(codec: &str) -> String {
    let codec = codec.to_uppercase();
    match codec.as_str() {
        "X264" | "H.264" | "AVC" => "H264".to_owned(),
        "X265" | "H.265" | "HEVC" => "H265".to_owned(),
        _ => codec,
    }
}

/// Normalizes audio codec string to a standard format
pub(super) fn normalize_audio_codec(codec: &str) -> String {
    let codec = codec.to_uppercase();
    if let Some(m) = AUDIO_NORMALIZE_RE.find(&codec) {
        let mut parts = vec![
            parse_audio_codec(&codec[..m.start()]),
            codec[m.start()..m.end()].to_owned(),
        ];

        let left = parse_audio_codec(&codec[m.end()..]);
        if !left.is_empty() {
            parts.push(left);
        }

        parts.join(".")
    } else {
        parse_audio_codec(&codec)
    }
}

/// Helper function to parse audio codec
fn parse_audio_codec(codec: &str) -> String {
    if codec.is_empty() {
        return codec.to_owned();
    }

    let mut parts = Vec::new();
    let val = codec.replace(" ", ".");
    for p in val.split('.') {
        let p = p.trim();
        if p.is_empty() {
            continue;
        }

        let p = match p {
            "TRUEHD" => "TrueHD",
            "ATMOS" => "Atmos",
            "DTSHD" => "DTS-HD",
            _ => p,
        };
        parts.push(p.to_owned());
    }

    parts.join(".")
}

/// Normalizes HDR string to a standard format
pub(super) fn normalize_hdr(hdr: &str) -> String {
    let hdr = hdr.to_uppercase().replace("-", "");
    if hdr.contains("DOLBY") || hdr == "DOVI" {
        "DV".to_owned()
    } else {
        hdr
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_quality() {
        assert_eq!(normalize_quality("web-dl"), "WEB-DL");
        assert_eq!(normalize_quality("webdl"), "WEB-DL");
        assert_eq!(normalize_quality("web-rip"), "WEBRip");
        assert_eq!(normalize_quality("webrip"), "WEBRip");
        assert_eq!(normalize_quality("bluray"), "BluRay");
        assert_eq!(normalize_quality("blu-ray"), "BluRay");
        assert_eq!(normalize_quality("bdrip"), "BDRip");
        assert_eq!(normalize_quality("bd-rip"), "BDRip");
        assert_eq!(normalize_quality("brrip"), "BRRip");
        assert_eq!(normalize_quality("br-rip"), "BRRip");
        assert_eq!(normalize_quality("remux"), "Remux");
        assert_eq!(normalize_quality("unknown"), "unknown");
    }

    #[test]
    fn test_normalize_video_codec() {
        assert_eq!(normalize_video_codec("X264"), "H264");
        assert_eq!(normalize_video_codec("H.264"), "H264");
        assert_eq!(normalize_video_codec("AVC"), "H264");
        assert_eq!(normalize_video_codec("X265"), "H265");
        assert_eq!(normalize_video_codec("H.265"), "H265");
        assert_eq!(normalize_video_codec("HEVC"), "H265");
        assert_eq!(normalize_video_codec("AV1"), "AV1");
    }

    #[test]
    fn test_normalize_audio_codec() {
        assert_eq!(normalize_audio_codec("TRUEHD"), "TrueHD");
        assert_eq!(normalize_audio_codec("TRUEHD.7.1"), "TrueHD.7.1");
        assert_eq!(
            normalize_audio_codec("TRUEHD.7.1.ATMOS"),
            "TrueHD.7.1.Atmos"
        );
        assert_eq!(normalize_audio_codec("DTS-HD.MA.7.1"), "DTS-HD.MA.7.1");
        assert_eq!(normalize_audio_codec("DTSHD.MA.7.1"), "DTS-HD.MA.7.1");
    }

    #[test]
    fn test_normalize_hdr() {
        assert_eq!(normalize_hdr("DOLBY"), "DV");
        assert_eq!(normalize_hdr("DOLBY-VISION"), "DV");
        assert_eq!(normalize_hdr("DOVI"), "DV");
        assert_eq!(normalize_hdr("HDR10"), "HDR10");
        assert_eq!(normalize_hdr("HDR10+"), "HDR10+");
    }
}
