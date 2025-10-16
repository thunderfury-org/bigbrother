use std::sync::LazyLock;
use std::{collections::HashSet, ops::Range};

use lingua::{Language, LanguageDetector, LanguageDetectorBuilder};
use regex::Regex;

use super::{MediaFileType, MediaInfo, normalize::*};

const RE_BEGIN: &str = r"(?i)[\. \-\[\{\(@]\s*";
const RE_END: &str = r"\s*[\. \-\]\}\)@]";

static TMDB_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(&format!("{}{}{}", RE_BEGIN, r"tmdb(?:id)?[-=](?P<value>\d+)", RE_END)).unwrap());
static FRAME_RATE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(&format!("{}{}{}", RE_BEGIN, r"(?P<value>\d{2,3}fps)", RE_END)).unwrap());
static QUALITY_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(&format!(
        "{}{}{}",
        RE_BEGIN, r"(?P<value>WEB-?DL|Blu-?Ray[\.\s-]?(?:Remux)?|Remux|WEB-?Rip|BR-?Rip|BD-?Rip)", RE_END
    ))
    .unwrap()
});
static HDR_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(&format!(
        "{}{}{}",
        RE_BEGIN, r"(?P<value>HDR(10\+?)?|Dolby[ -]?Vision|HLG|DV|DoVi)", RE_END
    ))
    .unwrap()
});
static VIDEO_CODEC_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(&format!(
        "{}{}{}",
        RE_BEGIN, r"(?P<value>[HX]\.?26[45]|AVC|HEVC|AV1|VP-9)", RE_END
    ))
    .unwrap()
});
static AUDIO_CODEC_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(&format!(
        "{}{}{}",
        RE_BEGIN,
        r"(?P<value>(?:AAC|FLAC|Dolby[\.\s]?Digital|DDP?|DTS(?:-?HD)?|TrueHD)(?:[\.\s]?(?:Atmos|MA|DDP?|\d\.\d))*)",
        RE_END
    ))
    .unwrap()
});
static RESOLUTION_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(&format!(
        "{}{}{}",
        RE_BEGIN, r"((\d{3,4}x(?P<height>\d{3,4}))|(?P<resolution>\d{1,4}[pk]))", RE_END
    ))
    .unwrap()
});
static YEAR_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(&format!("{}{}{}", RE_BEGIN, r"(?P<year>19\d{2}|20\d{2})", RE_END)).unwrap());
static SEASON_EPISODE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)([\.\s\[]S(?:eason)?\s*(?P<season_number>\d{1,2})\s*\]?\s*)([E#-\[]\s*(?P<episode_number>\d{1,4})(-(?P<episode_number2>\d{1,4}))?)?")
        .unwrap()
});
static EPISODE_ONLY_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(&format!(
        "{}{}{}",
        RE_BEGIN, r"[#第E]?\s*(?P<episode_number>\d{1,4})(-(?P<episode_number2>\d{1,4}))?\s*[集]?", RE_END
    ))
    .unwrap()
});
static RELEASE_GROUP_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\[\s*(?P<value>[^\[\]]+)\s*\]").unwrap());

static NAME_NORMALIZE_RE: LazyLock<Vec<Regex>> = LazyLock::new(|| {
    vec![
        Regex::new(r"[_（）《》]").unwrap(),
        Regex::new(r"[\[★](\S{1,4}年)?\S{1,2}月新番[\]★]").unwrap(),
        Regex::new(r"(?i)\[\d+(\.\d+)G?\]").unwrap(),
        Regex::new(r"(?i)10-?bit").unwrap(),
    ]
});
static TITLE_REPLACE_RE: LazyLock<Vec<(Regex, &'static str)>> = LazyLock::new(|| {
    vec![
        (Regex::new(r"[.\[\]{}\(\)]").unwrap(), " "),
        (Regex::new(r"第[^.\[\]]+季").unwrap(), ""),
    ]
});
static DIGIT_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\d+$").unwrap());

static VIDEO_EXTENSIONS: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
    let extensions = [
        ".3g2", ".3gp", ".3gp2", ".asf", ".avi", ".divx", ".flv", ".iso", ".m4v", ".mk2", ".mk3d", ".mka", ".mkv",
        ".mov", ".mp4", ".mp4a", ".mpeg", ".mpg", ".ogg", ".ogm", ".ogv", ".qt", ".ra", ".ram", ".rm", ".ts", ".m2ts",
        ".vob", ".wav", ".webm", ".wma", ".wmv",
    ];
    HashSet::from(extensions)
});
static SUBTITLE_EXTENSIONS: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
    let extensions = [".srt", ".sub", ".idx", ".ass", ".ssa"];
    HashSet::from(extensions)
});

static LANG_MAP: LazyLock<Vec<(&'static str, Vec<&'static str>)>> = LazyLock::new(|| {
    vec![
        (super::LANGUAGE_CHINESE_SIMPLIFIED, vec!["简", "chs", "gb", "zh-hans"]),
        (
            super::LANGUAGE_CHINESE_TRADITIONAL,
            vec!["繁", "cht", "big5", "zh-hant"],
        ),
    ]
});

static LANG_DETECTOR: LazyLock<LanguageDetector> = LazyLock::new(|| {
    let languages = vec![Language::English, Language::Chinese, Language::Japanese];
    LanguageDetectorBuilder::from_languages(&languages).build()
});

struct MediaInfoParser {
    name: String,
    other: String,

    title_index_end: Option<usize>,
    year_index_start: Option<usize>,

    info: MediaInfo,
}

impl MediaInfoParser {
    fn new(name: &str) -> Self {
        Self {
            name: name.to_owned(),
            other: String::new(),
            title_index_end: None,
            year_index_start: None,
            info: MediaInfo::default(),
        }
    }

    fn update_title_index_end(&mut self, index: usize) {
        if self.title_index_end.is_none() {
            self.title_index_end = Some(index);
        } else if index < self.title_index_end.unwrap() {
            self.title_index_end = Some(index);
        }
    }

    fn update_name_and_index(&mut self, match_range: Range<usize>) {
        let start = match_range.start;
        let end = match_range.end;
        self.name = format!("{}{}", &self.name[..start + 1], &self.name[end - 1..]);
        self.update_title_index_end(start + 1);
    }

    fn parse_value_from_name(&mut self, re: &Regex, normalizer: Option<fn(&str) -> String>) -> Option<String> {
        if let Some(caps) = re.captures_iter(&self.name).last() {
            if let Some(value_match) = caps.name("value") {
                let mut value = value_match.as_str().to_owned();
                self.update_name_and_index(caps.get_match().range());
                if let Some(norm) = normalizer {
                    value = norm(&value);
                }

                return if value.is_empty() { None } else { Some(value) };
            }
        }
        None
    }

    fn parse(&mut self) {
        self.normalize_name();

        self.info.tmdb_id = self.parse_value_from_name(&TMDB_RE, None);
        self.info.frame_rate = self.parse_value_from_name(&FRAME_RATE_RE, Some(|s| s.to_lowercase()));
        self.info.quality = self.parse_value_from_name(&QUALITY_RE, Some(normalize_quality));
        self.info.hdr = self.parse_value_from_name(&HDR_RE, Some(normalize_hdr));
        self.info.video_codec = self.parse_value_from_name(&VIDEO_CODEC_RE, Some(normalize_video_codec));
        self.info.audio_codec = self.parse_value_from_name(&AUDIO_CODEC_RE, Some(normalize_audio_codec));

        self.parse_resolution();
        self.parse_year();
        self.parse_season_episode();
        self.parse_file_type();
        self.parse_title();
        self.parse_subtitles();
        self.parse_release_group();
    }

    fn normalize_name(&mut self) {
        self.name = self.name.replace("【", "[");
        self.name = self.name.replace("】", "]");
        self.name = self.name.replace("精校", ".");

        for re in NAME_NORMALIZE_RE.iter() {
            self.name = re.replace_all(&self.name, ".").into_owned();
        }
        self.name = format!(" {} ", self.name);
    }

    fn parse_resolution(&mut self) {
        if let Some(caps) = RESOLUTION_RE.captures_iter(&self.name).last() {
            if let Some(height_match) = caps.name("height") {
                self.info.resolution = Some(format!("{}p", height_match.as_str()));
                self.update_name_and_index(caps.get_match().range());
            } else if let Some(res_match) = caps.name("resolution") {
                let mut res = res_match.as_str().to_lowercase();
                if res == "4k" {
                    res = "2160p".to_owned();
                }
                self.info.resolution = Some(res);
                self.update_name_and_index(caps.get_match().range());
            }
        }
    }

    fn parse_year(&mut self) {
        println!("name: {}", self.name);
        if let Some(caps) = YEAR_RE.captures_iter(&self.name).last() {
            if let Some(year_match) = caps.name("year") {
                if !caps.get_match().as_str().ends_with(')') {
                    // matched .year. but maybe it's part of the title, e.g. "Movie.Title.2020.2021"
                    // try to match another year
                    let new_name = &self.name[year_match.end() - 1..];
                    if let Some(caps2) = YEAR_RE.captures_iter(new_name).last() {
                        if let Some(year_match2) = caps2.name("year") {
                            self.info.year = Some(year_match2.as_str().to_owned());
                            self.year_index_start = Some(year_match.end() - 1 + caps2.get_match().start());
                            self.update_name_and_index(Range {
                                start: year_match.end() - 1 + caps2.get_match().start(),
                                end: year_match.end() - 1 + caps2.get_match().end(),
                            });
                            return;
                        }
                    }

                    // not matched, keep the original match
                }

                self.info.year = Some(year_match.as_str().to_owned());
                self.year_index_start = Some(caps.get_match().start());
                self.update_name_and_index(caps.get_match().range());
            }
        }
    }

    fn parse_season_episode(&mut self) {
        let mut caps = SEASON_EPISODE_RE.captures_iter(&self.name).last();
        if caps.is_none() {
            // not found season/episode info like S01E01
            // try match only episode info like 01 or - 01 or #01
            caps = EPISODE_ONLY_RE.captures_iter(&self.name).last();
            if caps.is_none()
                || (self.year_index_start.is_some()
                    && caps.as_ref().unwrap().get_match().start() < self.year_index_start.unwrap())
            {
                // season/episode info not found
                // or episode is before year, maybe it's title info
                if self.title_index_end.is_none() {
                    return;
                }

                // try to split name and other info by title index
                let split_at = self.title_index_end.unwrap();
                self.other = self.name[split_at..].to_owned();
                self.name = self.name[..split_at].to_owned();
                return;
            }
        }

        if let Some(caps) = caps {
            if let Some(season_match) = caps.name("season_number") {
                self.info.season_number = Some(season_match.as_str().parse().unwrap_or(0));
            }
            if let Some(ep_match) = caps.name("episode_number") {
                self.info.episode_number = Some(ep_match.as_str().parse().unwrap_or(0));
            }
            if let Some(ep2_match) = caps.name("episode_number2") {
                self.info.second_episode_number = Some(ep2_match.as_str().parse().unwrap_or(0));
            }

            let start = caps.get(0).unwrap().start();
            let end = caps.get(0).unwrap().end();
            self.other = format!(".{}", &self.name[end..]);
            self.name = self.name[..start].to_owned();
        }
    }

    fn parse_file_type(&mut self) {
        // find from the last dot
        if let Some(dot_idx) = self.other.rfind('.') {
            let ext = self.other[dot_idx..].trim().to_lowercase();
            if ext.len() > 1 {
                if VIDEO_EXTENSIONS.contains(&ext.as_str()) {
                    self.info.file_type = Some(MediaFileType::Video);
                } else if SUBTITLE_EXTENSIONS.contains(&ext.as_str()) {
                    self.info.file_type = Some(MediaFileType::Subtitle);
                } else {
                    // unknown file type
                    return;
                }
                self.info.extension = Some(ext);
                self.other = self.other[..dot_idx].to_owned();
            }
        }
    }

    fn parse_title(&mut self) {
        if let Some(idx) = self.name.find(']') {
            // remove [group] at the start of the name
            let mut name = self.name[idx + 1..].to_owned();
            for (re, to) in TITLE_REPLACE_RE.iter() {
                name = re.replace_all(&name, *to).into_owned();
            }

            let left = name.trim();
            if !left.is_empty() {
                self.info.release_group = Some(self.name[..idx].replace('[', "").trim().to_owned());
                self.name = left.to_owned();
            }
        }

        for (re, to) in TITLE_REPLACE_RE.iter() {
            self.name = re.replace_all(&self.name, *to).into_owned();
        }

        let mut titles = Vec::new();
        for part in self.name.split('/') {
            let part = part.trim();
            if part.is_empty() {
                continue;
            }
            if DIGIT_RE.is_match(part) {
                titles.push(super::MediaTitle {
                    language: super::LANGUAGE_ENGLISH.to_owned(),
                    title: part.to_owned(),
                });
                continue;
            }

            for r in LANG_DETECTOR.detect_multiple_languages_of(part) {
                titles.push(super::MediaTitle {
                    language: normalize_language(r.language()),
                    title: part[r.start_index()..r.end_index()].trim().to_owned(),
                });
            }
        }

        if !titles.is_empty() {
            self.info.titles = Some(titles);
        }
    }

    fn parse_subtitles(&mut self) {
        if self.other.is_empty() {
            return;
        }

        let mut subtitles: Vec<String> = Vec::new();
        let name = self.other.to_lowercase();
        for (lang, keywords) in LANG_MAP.iter() {
            for kw in keywords {
                if name.contains(kw) {
                    subtitles.push((*lang).to_owned());
                    break;
                }
            }
        }

        if !subtitles.is_empty() {
            self.info.subtitles = Some(subtitles);
        }
    }

    fn parse_release_group(&mut self) {
        if self.info.release_group.is_some() {
            return;
        }

        if let Some(caps) = RELEASE_GROUP_RE.captures_iter(&self.other).last() {
            if let Some(val_match) = caps.name("value") {
                let value = val_match.as_str().trim();
                if value.contains('-') {
                    self.info.release_group = Some(value.to_owned());
                    return;
                }
            }
        }

        if let Some(idx) = self.other.rfind('-') {
            self.info.release_group = Some(self.other[idx + 1..].trim().to_owned());
        }
    }
}

pub fn parse(name: &str) -> MediaInfo {
    let mut parser = MediaInfoParser::new(name);
    parser.parse();
    parser.info
}

#[cfg(test)]
mod tests {
    use std::fs;

    use serde::Deserialize;

    use super::*;

    #[derive(Deserialize)]
    struct TestCase {
        input: String,
        expected: MediaInfo,
    }

    #[test]
    fn test_parse_media() {
        let base_path = std::path::Path::new(file!()).parent().unwrap();

        let files = vec![
            "anime.yaml",
            "dir.yaml",
            "movie.yaml",
            "tv_episode.yaml",
            "tv_season_episode.yaml",
        ];

        for file in files {
            let content = fs::read_to_string(format!("{}/testdata/{}", base_path.display(), file)).unwrap();
            let cases: Vec<TestCase> = serde_yaml::from_str(&content).unwrap();
            for case in &cases {
                let info = parse(case.input.as_str());
                assert_eq!(case.expected, info, "input: {}", case.input);
            }
        }
    }
}
