//! Token-based media-filename parser.
//!
//! The parser tokenises the filename once, classifies each token via the
//! `labels` dictionary plus small per-token pattern matchers, then walks the
//! labeled token stream to fill in `Metadata`. This module is the top-level
//! coordinator: it owns `Metadata` and drives the deep modules in order.

pub(super) mod extractors;
pub(super) mod labels;
pub(super) mod release_group;
pub(super) mod title_resolver;
pub(super) mod tokenizer;

use std::collections::HashSet;
use std::ops::Range;
use std::sync::LazyLock;

use regex::Regex;

use super::{FileType, MediaKind, Metadata};
use extractors::{
    apply_basic_extractors_and_collect_spans, backfill_from_noise_brackets,
    extract_digit_only_episode, extract_episode_and_season, extract_frame_rate, extract_hdr,
    extract_quality, extract_resolution, extract_subtitle_suffix_span, extract_video_codec,
    extract_year, label_tokens, merge_split_codecs,
};
use labels::Label;
use release_group::extract_release_group;
use title_resolver::{extract_titles, resolve_unknown_neighbors};
use tokenizer::tokenize;

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

static NAME_NORMALIZE_RE: LazyLock<Vec<Regex>> = LazyLock::new(|| {
    vec![
        Regex::new(r"[_（）《》]").unwrap(),
        Regex::new(r"[\[★](\S{1,4}年)?\S{1,2}月新番[\]★]").unwrap(),
        Regex::new(r"(?i)\[\d+(\.\d+)G?\]").unwrap(),
        Regex::new(r"(?i)10-?bit").unwrap(),
    ]
});

pub(crate) fn parse(name: &str) -> Box<Metadata> {
    let (body, file_type, extension) = normalize_name(name);
    let mut metadata = Metadata {
        file_type,
        extension,
        ..Default::default()
    };

    let mut tokens = tokenize(&body);
    let mut noise_spans: Vec<Range<usize>> = Vec::new();
    label_tokens(&mut tokens);
    merge_split_codecs(&mut tokens);
    backfill_from_noise_brackets(&mut tokens, &mut metadata);

    extract_year(&mut tokens, &mut metadata);
    extract_resolution(&mut tokens, &mut metadata);
    extract_quality(&mut tokens, &mut metadata);
    extract_video_codec(&mut tokens, &mut metadata);
    extract_hdr(&mut tokens, &mut metadata);
    extract_frame_rate(&mut tokens, &mut metadata);

    // Body-level regex extractors for fields the token pipeline does not yet
    // fully own (TMDB id, multi-token audio codec, multi-token quality).
    let body_spans = apply_basic_extractors_and_collect_spans(&body, &mut metadata);
    for span in body_spans {
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

pub(super) fn span_fully_covers(outer: &Range<usize>, inner: &Range<usize>) -> bool {
    outer.start <= inner.start && outer.end >= inner.end
}

pub(super) fn spans_overlap(a: &Range<usize>, b: &Range<usize>) -> bool {
    a.start < b.end && b.start < a.end
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
    use std::{fs, path::Path};

    use serde::Deserialize;

    use crate::domain::media::{FileType, Metadata};

    #[derive(Deserialize)]
    struct TestCase {
        input: String,
        expected: Metadata,
    }

    #[test]
    fn test_parse_media() {
        let data_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src")
            .join("domain")
            .join("media")
            .join("testdata");

        let files = [
            "anime.yaml",
            "dir.yaml",
            "movie.yaml",
            "tv_episode.yaml",
            "tv_season_episode.yaml",
        ];

        for file in files {
            let content = fs::read_to_string(data_path.join(file)).unwrap();
            let cases: Vec<TestCase> = serde_yaml::from_str(&content).unwrap();
            for case in &cases {
                let info = super::parse(case.input.as_str());
                let mut expected = case.expected.clone();
                expected.media_kind = info.media_kind.clone();
                assert_eq!(expected, *info, "input: {}", case.input);
            }
        }
    }

    #[test]
    fn extracts_simple_movie_metadata_via_token_pipeline() {
        let meta = super::parse("Movie.2024.1080p.WEB-DL.H.264.AAC.mkv");

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
        let meta = super::parse("Movie.2024.4K.HDR10+.Dolby.Vision.WEB-DL");

        assert_eq!(meta.resolution, "2160p");
        assert_eq!(meta.hdr, "DV");
    }

    #[test]
    fn extracts_frame_rate_token() {
        let meta = super::parse("Show.S01E02.1080p.60fps.WEB-DL");

        assert_eq!(meta.frame_rate, "60fps");
        assert_eq!(meta.resolution, "1080p");
    }
}
