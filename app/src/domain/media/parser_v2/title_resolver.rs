use lingua::{Language, LanguageDetector};

use super::super::normalize::normalize_language;
use super::super::{LANGUAGE_CHINESE, LANGUAGE_ENGLISH, LANGUAGE_JAPANESE, Title};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ScriptClass {
    Chinese,
    Japanese,
    Latin,
    Mixed,
    Other,
}

pub(crate) fn titles_from_cleaned_part(value: &str, detector: &LanguageDetector) -> Vec<Title> {
    let mut titles = Vec::new();

    for segment in split_title_segments(value) {
        if segment.chars().all(|ch| ch.is_ascii_digit()) {
            titles.push(Title {
                language: LANGUAGE_ENGLISH.to_owned(),
                title: segment,
            });
            continue;
        }

        if let Some(title) = title_from_script_segment(segment.as_str()) {
            titles.push(title);
            continue;
        }

        for language in detector.detect_multiple_languages_of(segment.as_str()) {
            let piece = segment[language.start_index()..language.end_index()].trim();
            if piece.is_empty() {
                continue;
            }

            let title = if language.language() == Language::Chinese {
                piece.replace('-', "").trim().to_owned()
            } else {
                piece.to_owned()
            };
            if title.is_empty() {
                continue;
            }

            titles.push(Title {
                language: normalize_language(language.language()),
                title,
            });
        }
    }

    titles
}

pub(crate) fn rebalance_title_boundaries(titles: &mut [Title]) {
    for index in 1..titles.len() {
        if !matches!(titles[index - 1].language.as_str(), "zh" | "jp") {
            continue;
        }
        if titles[index].language != LANGUAGE_ENGLISH {
            continue;
        }

        loop {
            let trimmed = titles[index].title.trim_start().to_owned();
            let Some(rest) = trimmed.strip_prefix('～') else {
                break;
            };
            titles[index - 1].title.push('～');
            titles[index].title = rest.trim_start().to_owned();
        }
    }
}

pub(crate) fn prefix_contains_title(value: &str) -> bool {
    value
        .chars()
        .any(|ch| ch.is_alphabetic() || ('\u{4e00}'..='\u{9fff}').contains(&ch))
}

fn split_title_segments(value: &str) -> Vec<String> {
    let mut segments = Vec::new();
    let mut current = String::new();
    let mut current_script = ScriptClass::Other;

    for token in value.split_whitespace() {
        let script = classify_script(token);
        if current.is_empty() {
            current.push_str(token);
            current_script = script;
            continue;
        }

        if should_split_script_segment(current_script, script) {
            segments.push(current);
            current = token.to_owned();
            current_script = script;
            continue;
        }

        current.push(' ');
        current.push_str(token);
        current_script = merge_script_class(current_script, script);
    }

    if !current.is_empty() {
        segments.push(current);
    }

    if segments.is_empty() {
        vec![value.trim().to_owned()]
    } else {
        segments
    }
}

fn title_from_script_segment(segment: &str) -> Option<Title> {
    let title = segment.trim();
    if title.is_empty() {
        return None;
    }

    let language = match classify_script(title) {
        ScriptClass::Chinese => LANGUAGE_CHINESE,
        ScriptClass::Japanese => LANGUAGE_JAPANESE,
        ScriptClass::Latin => LANGUAGE_ENGLISH,
        ScriptClass::Mixed | ScriptClass::Other => return None,
    };

    Some(Title {
        title: title.to_owned(),
        language: language.to_owned(),
    })
}

fn should_split_script_segment(left: ScriptClass, right: ScriptClass) -> bool {
    matches!(
        (left, right),
        (ScriptClass::Chinese, ScriptClass::Latin)
            | (ScriptClass::Latin, ScriptClass::Chinese)
            | (ScriptClass::Japanese, ScriptClass::Latin)
            | (ScriptClass::Latin, ScriptClass::Japanese)
    )
}

fn merge_script_class(left: ScriptClass, right: ScriptClass) -> ScriptClass {
    if left == right {
        return left;
    }
    if left == ScriptClass::Other {
        return right;
    }
    if right == ScriptClass::Other {
        return left;
    }

    ScriptClass::Mixed
}

fn classify_script(value: &str) -> ScriptClass {
    let mut has_han = false;
    let mut has_kana = false;
    let mut has_latin = false;

    for ch in value.chars() {
        if ch.is_ascii_alphabetic() {
            has_latin = true;
            continue;
        }
        if ('\u{4e00}'..='\u{9fff}').contains(&ch) {
            has_han = true;
            continue;
        }
        if ('\u{3040}'..='\u{30ff}').contains(&ch) {
            has_kana = true;
        }
    }

    match (has_han, has_kana, has_latin) {
        (true, false, false) => ScriptClass::Chinese,
        (false, true, false) | (true, true, false) => ScriptClass::Japanese,
        (false, false, true) => ScriptClass::Latin,
        (false, false, false) => ScriptClass::Other,
        _ => ScriptClass::Mixed,
    }
}

#[cfg(test)]
mod tests {
    use std::sync::LazyLock;

    use lingua::{Language, LanguageDetector, LanguageDetectorBuilder};

    use super::*;

    static DETECTOR: LazyLock<LanguageDetector> = LazyLock::new(|| {
        LanguageDetectorBuilder::from_languages(&[
            Language::English,
            Language::Chinese,
            Language::Japanese,
        ])
        .build()
    });

    #[test]
    fn splits_cjk_and_latin_titles() {
        let titles = titles_from_cleaned_part("Bofuri 因为太怕痛就全点防御力了。", &DETECTOR);

        assert_eq!(
            titles,
            vec![
                Title {
                    title: "Bofuri".to_owned(),
                    language: LANGUAGE_ENGLISH.to_owned(),
                },
                Title {
                    title: "因为太怕痛就全点防御力了。".to_owned(),
                    language: LANGUAGE_CHINESE.to_owned(),
                },
            ]
        );
    }

    #[test]
    fn keeps_single_script_segments_as_one_title() {
        let titles = titles_from_cleaned_part("名侦探柯南", &DETECTOR);

        assert_eq!(
            titles,
            vec![Title {
                title: "名侦探柯南".to_owned(),
                language: LANGUAGE_CHINESE.to_owned(),
            }]
        );
    }

    #[test]
    fn rebalances_wave_dash_into_cjk_title() {
        let mut titles = vec![
            Title {
                title: "异世界一击无双姐姐".to_owned(),
                language: LANGUAGE_CHINESE.to_owned(),
            },
            Title {
                title: "～ Isekai One Turn Kill Neesan".to_owned(),
                language: LANGUAGE_ENGLISH.to_owned(),
            },
        ];

        rebalance_title_boundaries(&mut titles);

        assert_eq!(titles[0].title, "异世界一击无双姐姐～");
        assert_eq!(titles[1].title, "Isekai One Turn Kill Neesan");
    }

    #[test]
    fn detects_title_prefix_in_cjk_and_latin_text() {
        assert!(prefix_contains_title("Perfect Crown"));
        assert!(prefix_contains_title("名侦探柯南"));
        assert!(!prefix_contains_title("2024 / 1080"));
    }
}
