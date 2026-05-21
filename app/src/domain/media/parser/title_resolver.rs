use std::ops::Range;
use std::sync::LazyLock;

use lingua::{Language, LanguageDetector, LanguageDetectorBuilder};
use regex::Regex;

use super::super::normalize::normalize_language;
use super::super::{LANGUAGE_CHINESE, LANGUAGE_ENGLISH, LANGUAGE_JAPANESE, Metadata, Title};
use super::labels::Label;
use super::tokenizer::{Token, TokenKind};

static TITLE_REPLACE_RE: LazyLock<Vec<(Regex, &'static str)>> = LazyLock::new(|| {
    vec![
        (Regex::new(r"[\.\[\]\{\}\(\)]").unwrap(), " "),
        (Regex::new(r"第[^.\[\]]+季").unwrap(), ""),
        (Regex::new(r"\s+-\s+").unwrap(), " "),
    ]
});
static TITLE_TRIM_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\s{2,}").unwrap());

static LANG_DETECTOR: LazyLock<LanguageDetector> = LazyLock::new(|| {
    let languages = vec![Language::English, Language::Chinese, Language::Japanese];
    LanguageDetectorBuilder::from_languages(&languages).build()
});

// An Unknown bare token sandwiched between two noise-labeled tokens, whose
// own text looks like a typical ASCII scene tag (short, all-caps, no spaces),
// is relabeled as SourceTag so it drops out of the title candidate. CJK
// content, multi-word phrases, and longer ASCII strings keep their Unknown
// label and are later folded into the title.
pub(super) fn resolve_unknown_neighbors(tokens: &mut [Token]) {
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

pub(super) fn extract_titles(
    body: &str,
    tokens: &[Token],
    noise_spans: &[Range<usize>],
    metadata: &mut Metadata,
) {
    let cutoff = scene_episode_byte_cutoff(body, tokens).unwrap_or(body.len());
    let mut candidate = String::with_capacity(body.len());

    // Walk the body, but decide each byte's fate from token labels (and the
    // body-level noise span list, which covers substrings the body-regex
    // extractors marked inside a token). Tokens fully cover the body except
    // at structural separators (`.`, `_`, ` `, `/`), which we preserve so
    // multi-title `/` splitting still works downstream.
    for (idx, ch) in body.char_indices() {
        if idx >= cutoff {
            break;
        }
        if byte_is_title(idx, tokens, noise_spans) {
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

fn byte_is_title(byte_index: usize, tokens: &[Token], noise_spans: &[Range<usize>]) -> bool {
    if noise_spans
        .iter()
        .any(|span| span.start <= byte_index && byte_index < span.end)
    {
        return false;
    }
    // A byte outside every token span is a structural separator — keep it
    // (so multi-title `/` splits survive).
    let owning_token = tokens
        .iter()
        .find(|token| token.span.start <= byte_index && byte_index < token.span.end);
    match owning_token {
        None => true,
        Some(token) => matches!(token.label, Label::Unknown | Label::Title),
    }
}

// When the body has a scene-style `S\d+E\d+` marker and a title-like prefix
// precedes it, any tokens after the marker belong to the scene episode's own
// title and should not be folded into the main title.
fn scene_episode_byte_cutoff(body: &str, tokens: &[Token]) -> Option<usize> {
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

#[cfg(test)]
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
