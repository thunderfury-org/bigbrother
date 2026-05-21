pub(super) mod extractors;
pub(super) mod labels;
mod pipeline;
pub(super) mod release_group;
pub(super) mod title_resolver;
pub(super) mod tokenizer;

use super::Metadata;

pub(crate) fn parse(value: &str) -> Box<Metadata> {
    pipeline::parse(value)
}

#[cfg(test)]
mod tests {
    use std::{fs, path::Path};

    use serde::Deserialize;

    use crate::domain::media::Metadata;

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
}
