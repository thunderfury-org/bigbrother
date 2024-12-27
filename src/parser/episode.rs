use std::{collections::HashMap, vec};

use lazy_static::lazy_static;
use regex::{Captures, Regex};

use super::{lang, EpisodeInfo, FileType, Title};

impl From<&str> for EpisodeInfo {
    fn from(filename: &str) -> Self {
        let mut info = EpisodeInfo::default();

        info.parse_file_type(filename);

        let mut filename = nomalize_filename(filename);
        info.parse_resolution(&mut filename);
        info.parse_special_season_number(&mut filename);

        let res = info.parse_season_and_episode_number(&filename);
        if res.is_none() {
            return info;
        }

        let (title_part, extra_part) = res.unwrap();
        info.parse_title(title_part);
        info.parse_subtitles(extra_part);

        info
    }
}

impl EpisodeInfo {
    fn parse_file_type<'a>(&mut self, filename: &'a str) -> &'a str {
        match filename.rfind('.') {
            Some(i) => {
                let ext = &filename[i + 1..];
                self.file_type = ext.into();

                if self.file_type != FileType::Unknown {
                    self.extension = Some(ext.to_owned());

                    &filename[..i]
                } else {
                    filename
                }
            }
            None => filename,
        }
    }

    fn parse_resolution(&mut self, filename: &mut String) {
        lazy_static! {
            static ref RESOLUTION_RE: Regex =
                Regex::new(r"(\d{3,4}x(?P<height>\d{3,4}))|(?i)(?P<resolution>\d{1,4}[pk])").unwrap();
        }

        if let Some(caps) = RESOLUTION_RE.captures(filename) {
            if let Some(height) = caps.name("height") {
                self.resolution = Some(format!("{}p", height.as_str()));
            } else if let Some(resolution) = caps.name("resolution") {
                let mut resolution = resolution.as_str().to_lowercase();
                if resolution == "4k" {
                    resolution = "2160p".to_string();
                }
                self.resolution = Some(resolution);
            }

            filename.replace_range(caps.get(0).unwrap().range(), " ");
        }
    }

    fn parse_special_season_number(&mut self, filename: &mut String) {
        lazy_static! {
            static ref SPECIAL_SEASON_NUMBER_RE: Regex = Regex::new(r"第(?P<season_number>\d{1,3})季").unwrap();
        }

        if let Some(caps) = SPECIAL_SEASON_NUMBER_RE.captures(filename) {
            if let Some(season_number) = caps.name("season_number") {
                self.season_number = Some(season_number.as_str().parse().unwrap());
            }

            filename.replace_range(caps.get(0).unwrap().range(), "");
        }
    }

    fn parse_season_and_episode_number<'a>(&mut self, filename: &'a str) -> Option<(&'a str, &'a str)> {
        lazy_static! {
            static ref SEASON_AND_EPISODE_NUMBER_RE: Regex =
                Regex::new(r"(?i)(\[?S(eason)?\s*(?P<season_number>\d{1,2})\s*\]?\s*)?([\[|E]|(\-\s+)|(#\s*))(?P<episode_number>\d{1,4})(-(?P<episode_number2>\d{1,4}))?").unwrap();

            static ref SIMPLE_EPISODE_NUMBER_RE: Regex = Regex::new(r"(?P<episode_number>\d{1,4})").unwrap();
        }

        if let Some(p) = self.try_parse_season_and_episode_number(filename) {
            Some(p)
        } else if let Some(caps) = SIMPLE_EPISODE_NUMBER_RE.captures(filename) {
            if let Some(episode_number) = caps.name("episode_number") {
                self.episode_number = Some(episode_number.as_str().parse().unwrap());
            }

            let m = caps.get(0).unwrap();
            Some((filename[..m.start()].trim(), filename[m.end()..].trim()))
        } else {
            None
        }
    }

    fn try_parse_season_and_episode_number<'a>(&mut self, filename: &'a str) -> Option<(&'a str, &'a str)> {
        lazy_static! {
            static ref SEASON_AND_EPISODE_NUMBER_RE: Regex =
                Regex::new(r"(?i)(\[?S(eason)?\s*(?P<season_number>\d{1,2})\s*\]?\s*)?([\[|E]|(\-\s+)|(#\s*))(?P<episode_number>\d{1,4})(-(?P<episode_number2>\d{1,4}))?").unwrap();
        }

        let mut all_caps: Vec<Captures<'_>> = vec![];

        for caps in SEASON_AND_EPISODE_NUMBER_RE.captures_iter(filename) {
            self.season_number = None;
            self.episode_number = None;

            if let Some(season_number) = caps.name("season_number") {
                self.season_number = Some(season_number.as_str().parse().unwrap());
            }
            if caps.name("episode_number2").is_some() {
                // 不支持单个文件多集，或者合集资源
            } else if let Some(episode_number) = caps.name("episode_number") {
                self.episode_number = Some(episode_number.as_str().parse().unwrap());
            }

            if self.season_number.is_some() && self.episode_number.is_some() {
                let m = caps.get(0).unwrap();
                return Some((filename[..m.start()].trim(), filename[m.end()..].trim()));
            } else {
                all_caps.push(caps);
            }
        }

        all_caps.first().map(|c| {
            self.season_number = None;
            self.episode_number = None;

            if let Some(season_number) = c.name("season_number") {
                self.season_number = Some(season_number.as_str().parse().unwrap());
            }
            if c.name("episode_number2").is_some() {
                // 不支持单个文件多集，或者合集资源
            } else if let Some(episode_number) = c.name("episode_number") {
                self.episode_number = Some(episode_number.as_str().parse().unwrap());
            }

            let m = c.get(0).unwrap();
            (filename[..m.start()].trim(), filename[m.end()..].trim())
        })
    }

    fn parse_title(&mut self, filename: &str) {
        lazy_static! {
            static ref TITLE_RE: Regex = Regex::new(r"(\[(?P<release_group>[^\]]+)\])?\[?(?P<title>[^\]\[]+)").unwrap();
        }

        if filename.is_empty() {
            return;
        }

        if let Some(caps) = TITLE_RE.captures(filename) {
            if let Some(release_group) = caps.name("release_group") {
                self.release_group = Some(release_group.as_str().trim().to_string());
            }
            if let Some(title) = caps.name("title") {
                let titles = Title::parse(title.as_str());
                if !titles.is_empty() {
                    self.titles = Some(titles);
                }
            }
        }
    }

    fn parse_subtitles(&mut self, filename: &str) {
        lazy_static! {
            static ref LANG_MAP: HashMap<&'static str, Vec<&'static str>> = HashMap::from([
                (lang::LANG_ZH_CN, vec!["简", "chs", "gb", "zh-hans"]),
                (lang::LANG_ZH_TW, vec!["繁", "cht", "big5", "zh-hant"]),
                (lang::LANG_JP, vec!["日"]),
            ]);
        }

        let filename = filename.to_lowercase();
        let mut subtitles: Vec<String> = Vec::new();
        LANG_MAP.iter().for_each(|(key, value)| {
            for lang in value {
                if filename.contains(lang) {
                    subtitles.push(key.to_string());
                    break;
                }
            }
        });

        if !subtitles.is_empty() {
            subtitles.sort();
            self.subtitles = Some(subtitles);
        }
    }
}

fn nomalize_filename(filename: &str) -> String {
    lazy_static! {
        static ref NORMALIZE_FILENAME_RE_LIST: Vec<Regex> = vec![
            Regex::new(r"(?i)@?\d{2,3}\s*fps").unwrap(),
            Regex::new(r"第[^\d]+季").unwrap(),
            Regex::new(r"[\[★](\S{1,4}年)?\S{1,2}月新番[\]★]").unwrap(),
        ];
    }

    let mut n = filename
        .replace('【', "[")
        .replace('】', "]")
        .replace('(', "[")
        .replace(')', "]")
        .replace(['_', '。', '.'], " ");

    for re in NORMALIZE_FILENAME_RE_LIST.as_slice() {
        n = re.replace_all(&n, "").to_string();
    }

    n
}

#[cfg(test)]
mod tests {
    use std::fs;

    use serde::Deserialize;

    use super::*;

    #[derive(Deserialize)]
    struct TestCase {
        input: String,
        expected: EpisodeInfo,
    }

    #[test]
    fn test_parse_episode() {
        let content = fs::read_to_string(format!(
            "{}/resources/test/utils/filename_parser/tv.yaml",
            env!("CARGO_MANIFEST_DIR")
        ))
        .unwrap();

        let cases: Vec<TestCase> = serde_yaml::from_str(&content).unwrap();

        for case in &cases {
            let episode = EpisodeInfo::from(case.input.as_str());
            assert_eq!(case.expected, episode, "input: {}", case.input);
        }
    }
}
