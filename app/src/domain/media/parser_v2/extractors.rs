use regex::Regex;
use std::ops::Range;
use std::sync::LazyLock;

use super::super::Metadata;
use super::super::normalize::{
    normalize_audio_codec, normalize_hdr, normalize_quality, normalize_video_codec,
};

static TMDB_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)(?:^|[ ._\-\[\(\{])tmdb(?:id)?[-=](?P<value>\d+)").unwrap());
static FRAME_RATE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)(?P<value>\d{2,3}fps)").unwrap());
static QUALITY_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)(?P<value>WEB-?DL|WEB-?Rip|WEBRIP|Blu-?Ray(?:[ ._-]?Remux)?|Remux|BR-?Rip|BD-?Rip)",
    )
    .unwrap()
});
static HDR_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(?P<value>HDR10\+?|HDR|Dolby[ -]?Vision|HLG|DV|DoVi)").unwrap()
});
static VIDEO_CODEC_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)(?P<value>[HX]\.?(?:26[45])|AVC|HEVC|AV1|VP-9)").unwrap());
static AUDIO_CODEC_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)(?P<value>(?:AAC|FLAC|Dolby[.\s]?Digital|DDP?|DTS(?:[.\s-]?HD)?|TrueHD)(?:[.\s_-]?(?:Atmos|MA|DDP?|\d\.\d))*)",
    )
    .unwrap()
});
static RESOLUTION_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(?P<value>4k|\d{1,4}[pk]|\d{3,4}x(?P<height>\d{3,4}))").unwrap()
});
static YEAR_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)(?P<year>19\d{2}|20\d{2})").unwrap());

pub(crate) fn apply_basic_extractors_and_collect_spans(
    body: &str,
    metadata: &mut Metadata,
) -> Vec<Range<usize>> {
    let mut spans = Vec::new();

    if metadata.tmdb_id.is_empty()
        && let Some((span, tmdb_id)) = extract_tmdb_id_with_span(body)
    {
        metadata.tmdb_id = tmdb_id;
        spans.push(span);
    }
    if metadata.frame_rate.is_empty()
        && let Some((span, frame_rate)) = extract_frame_rate_with_span(body)
    {
        metadata.frame_rate = frame_rate;
        spans.push(span);
    }
    if metadata.quality.is_empty()
        && let Some((span, quality)) = extract_quality_with_span(body)
    {
        metadata.quality = quality;
        spans.push(span);
    }
    if metadata.hdr.is_empty()
        && let Some((span, hdr)) = extract_hdr_with_span(body)
    {
        metadata.hdr = hdr;
        spans.push(span);
    }
    if metadata.video_codec.is_empty()
        && let Some((span, video_codec)) = extract_video_codec_with_span(body)
    {
        metadata.video_codec = video_codec;
        spans.push(span);
    }
    if metadata.audio_codec.is_empty()
        && let Some((span, audio_codec)) = extract_audio_codec_with_span(body)
    {
        metadata.audio_codec = audio_codec;
        spans.push(span);
    }
    if metadata.resolution.is_empty()
        && let Some((span, resolution)) = extract_resolution_with_span(body)
    {
        metadata.resolution = resolution;
        spans.push(span);
    }
    if metadata.year.is_empty()
        && let Some((span, year)) = extract_year_with_span(body)
    {
        metadata.year = year;
        spans.push(span);
    }

    spans
}

fn extract_tmdb_id_with_span(body: &str) -> Option<(Range<usize>, String)> {
    TMDB_RE
        .captures_iter(body)
        .filter_map(|caps| {
            Some((
                caps.get(0)?.range(),
                caps.name("value")?.as_str().to_owned(),
            ))
        })
        .last()
}

fn extract_frame_rate_with_span(body: &str) -> Option<(Range<usize>, String)> {
    FRAME_RATE_RE
        .captures_iter(body)
        .filter_map(|caps| {
            Some((
                caps.get(0)?.range(),
                caps.name("value")?.as_str().to_lowercase(),
            ))
        })
        .last()
}

fn extract_quality_with_span(body: &str) -> Option<(Range<usize>, String)> {
    QUALITY_RE
        .captures_iter(body)
        .filter_map(|caps| {
            Some((
                caps.get(0)?.range(),
                normalize_quality(caps.name("value")?.as_str()),
            ))
        })
        .last()
}

fn extract_hdr_with_span(body: &str) -> Option<(Range<usize>, String)> {
    let mut selected = None;
    let mut fallback = None;

    for caps in HDR_RE.captures_iter(body) {
        let Some(value_match) = caps.name("value") else {
            continue;
        };
        let span = caps.get(0).unwrap().range();
        let normalized = normalize_hdr(value_match.as_str());
        if normalized == "DV" {
            selected = Some((span, normalized));
        } else {
            fallback = Some((span, normalized));
        }
    }

    selected.or(fallback)
}

fn extract_video_codec_with_span(body: &str) -> Option<(Range<usize>, String)> {
    VIDEO_CODEC_RE
        .captures_iter(body)
        .filter_map(|caps| {
            Some((
                caps.get(0)?.range(),
                normalize_video_codec(caps.name("value")?.as_str()),
            ))
        })
        .last()
}

fn extract_audio_codec_with_span(body: &str) -> Option<(Range<usize>, String)> {
    AUDIO_CODEC_RE
        .captures_iter(body)
        .filter_map(|caps| {
            Some((
                caps.get(0)?.range(),
                normalize_audio_codec(caps.name("value")?.as_str()),
            ))
        })
        .last()
}

fn extract_resolution_with_span(body: &str) -> Option<(Range<usize>, String)> {
    let mut selected = None;
    for caps in RESOLUTION_RE.captures_iter(body) {
        let span = caps.get(0).unwrap().range();
        if let Some(height_match) = caps.name("height") {
            selected = Some((span, format!("{}p", height_match.as_str())));
        } else if let Some(resolution_match) = caps.name("value") {
            let mut resolution = resolution_match.as_str().to_lowercase();
            if resolution == "4k" {
                resolution = "2160p".to_owned();
            }
            selected = Some((span, resolution));
        }
    }
    selected
}

fn extract_year_with_span(body: &str) -> Option<(Range<usize>, String)> {
    let mut years = Vec::new();
    for caps in YEAR_RE.captures_iter(body) {
        let Some(year_match) = caps.name("year") else {
            continue;
        };
        if !is_standalone_year(body, &year_match.range()) {
            continue;
        }
        let year = year_match.as_str().parse::<u32>().unwrap_or_default();
        if !(1900..=2099).contains(&year) {
            continue;
        }
        years.push((year_match.range(), year_match.as_str().to_owned()));
    }
    years.pop()
}

fn is_standalone_year(body: &str, span: &Range<usize>) -> bool {
    let prev = body[..span.start].chars().next_back();
    let next = body[span.end..].chars().next();

    is_year_boundary(prev) && is_year_boundary(next)
}

fn is_year_boundary(ch: Option<char>) -> bool {
    match ch {
        None => true,
        Some(value) => matches!(
            value,
            ' ' | '.' | '_' | '-' | '[' | ']' | '(' | ')' | '{' | '}'
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_tmdb_id() {
        assert_eq!(
            extract_tmdb_id_with_span("Movie.tmdb-12345.1080p").map(|(_, value)| value),
            Some("12345".to_owned())
        );
    }

    #[test]
    fn extracts_frame_rate() {
        assert_eq!(
            extract_frame_rate_with_span("Movie.60fps.1080p").map(|(_, value)| value),
            Some("60fps".to_owned())
        );
    }

    #[test]
    fn extracts_quality() {
        assert_eq!(
            extract_quality_with_span("Movie.1080p.BluRay.Remux").map(|(_, value)| value),
            Some("Remux".to_owned())
        );
    }

    #[test]
    fn extracts_hdr() {
        assert_eq!(
            extract_hdr_with_span("Movie.2160p.HDR10+.DoVi").map(|(_, value)| value),
            Some("DV".to_owned())
        );
    }

    #[test]
    fn extracts_video_codec() {
        assert_eq!(
            extract_video_codec_with_span("Movie.H.264.AAC").map(|(_, value)| value),
            Some("H264".to_owned())
        );
    }

    #[test]
    fn extracts_audio_codec() {
        assert_eq!(
            extract_audio_codec_with_span("Movie.DTS-HD.MA5.1.H.264").map(|(_, value)| value),
            Some("DTS-HD.MA.5.1".to_owned())
        );
    }

    #[test]
    fn extracts_resolution() {
        assert_eq!(
            extract_resolution_with_span("Movie.4K.WEB-DL").map(|(_, value)| value),
            Some("2160p".to_owned())
        );
        assert_eq!(
            extract_resolution_with_span("Movie.1920x1080.WEB-DL").map(|(_, value)| value),
            Some("1080p".to_owned())
        );
    }

    #[test]
    fn extracts_year() {
        assert_eq!(
            extract_year_with_span("Movie.2024.1080p").map(|(_, value)| value),
            Some("2024".to_owned())
        );
    }

    #[test]
    fn basic_extractors_collect_spans() {
        let mut metadata = Metadata::default();
        let spans = apply_basic_extractors_and_collect_spans(
            "Movie.2024.1080p.WEB-DL.H.264.AAC",
            &mut metadata,
        );

        assert_eq!(metadata.year, "2024");
        assert_eq!(metadata.resolution, "1080p");
        assert_eq!(metadata.quality, "WEB-DL");
        assert_eq!(metadata.video_codec, "H264");
        assert_eq!(metadata.audio_codec, "AAC");
        assert!(!spans.is_empty());
    }
}
