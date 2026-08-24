use std::time::Duration;

use serde::{Deserialize, Serialize};

use super::outcome::ImportOutcome;
use super::status::ImportStatus;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ImportSourceKind {
    Pan123,
    Pan189,
    Pan115,
    Telegram,
    FileIndex,
    #[serde(other)]
    Other,
}

impl ImportSourceKind {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Pan123 => "pan123",
            Self::Pan189 => "pan189",
            Self::Pan115 => "pan115",
            Self::Telegram => "telegram",
            Self::FileIndex => "file_index",
            Self::Other => "other",
        }
    }

    pub(crate) fn from_str(value: &str) -> Self {
        match value {
            "pan123" => Self::Pan123,
            "pan189" => Self::Pan189,
            "pan115" => Self::Pan115,
            "telegram" => Self::Telegram,
            "file_index" => Self::FileIndex,
            _ => Self::Other,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ImportSource {
    pub kind: ImportSourceKind,
    pub raw: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub(crate) enum SummaryItem {
    Movie {
        title: String,
        year: String,
        size: u64,
        cost_ms: u64,
        succeeded: bool,
    },
    Tv {
        name: String,
        year: String,
        season: u32,
        episodes: Vec<EpisodeOutcome>,
        missing_episodes: Vec<u32>,
        max_episode_number: u32,
        number_of_episodes: u32,
        total_size: u64,
        cost_ms: u64,
    },
    Skipped {
        files: Vec<String>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct EpisodeOutcome {
    pub episode: u32,
    pub succeeded: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub(crate) struct RecordSummary {
    pub items: Vec<SummaryItem>,
    pub total_size: u64,
    pub total_cost_ms: u64,
    pub skipped_files: Vec<String>,
}

impl RecordSummary {
    pub(crate) fn display_fields(&self) -> (Option<String>, Option<String>, Option<u64>) {
        match self.items.first() {
            Some(SummaryItem::Movie {
                title, year, size, ..
            }) => (Some(title.clone()), Some(year.clone()), Some(*size)),
            Some(SummaryItem::Tv {
                name,
                year,
                total_size,
                ..
            }) => (Some(name.clone()), Some(year.clone()), Some(*total_size)),
            Some(SummaryItem::Skipped { .. }) | None => (None, None, None),
        }
    }
}

fn duration_ms(value: Duration) -> u64 {
    u64::try_from(value.as_millis()).unwrap_or(u64::MAX)
}

pub(crate) fn summarize(items: &[ImportOutcome]) -> (RecordSummary, ImportStatus) {
    let mut summary = RecordSummary::default();
    let mut had_success = false;
    let mut had_failure = false;
    let mut had_non_skipped = false;
    let mut total_cost = Duration::ZERO;

    for item in items {
        match item {
            ImportOutcome::Movie {
                title,
                year,
                size,
                cost,
                has_failed,
            } => {
                had_non_skipped = true;
                if *has_failed {
                    had_failure = true;
                } else {
                    had_success = true;
                }
                summary.total_size = summary.total_size.saturating_add(*size);
                total_cost = total_cost.saturating_add(*cost);
                summary.items.push(SummaryItem::Movie {
                    title: title.clone(),
                    year: year.clone(),
                    size: *size,
                    cost_ms: duration_ms(*cost),
                    succeeded: !*has_failed,
                });
            }
            ImportOutcome::Tv {
                name,
                year,
                season,
                episodes,
                missing_episodes,
                failed_episodes,
                max_episode_number,
                number_of_episodes,
                total_size,
                cost,
                has_failed,
            } => {
                had_non_skipped = true;
                let mut combined: Vec<EpisodeOutcome> = episodes
                    .iter()
                    .map(|episode| EpisodeOutcome {
                        episode: *episode,
                        succeeded: true,
                    })
                    .collect();
                combined.extend(failed_episodes.iter().map(|episode| EpisodeOutcome {
                    episode: *episode,
                    succeeded: false,
                }));
                combined.sort_by_key(|outcome| outcome.episode);

                if !episodes.is_empty() {
                    had_success = true;
                }
                if *has_failed {
                    had_failure = true;
                }

                summary.total_size = summary.total_size.saturating_add(*total_size);
                total_cost = total_cost.saturating_add(*cost);
                summary.items.push(SummaryItem::Tv {
                    name: name.clone(),
                    year: year.clone(),
                    season: *season,
                    episodes: combined,
                    missing_episodes: missing_episodes.clone(),
                    max_episode_number: *max_episode_number,
                    number_of_episodes: *number_of_episodes,
                    total_size: *total_size,
                    cost_ms: duration_ms(*cost),
                });
            }
            ImportOutcome::Skipped { files } => {
                summary.skipped_files.extend(files.clone());
                summary.items.push(SummaryItem::Skipped {
                    files: files.clone(),
                });
            }
        }
    }

    summary.total_cost_ms = duration_ms(total_cost);

    let status = if !had_non_skipped {
        ImportStatus::Skipped
    } else if had_success && had_failure {
        ImportStatus::PartiallyFailed
    } else if had_failure {
        ImportStatus::Failed
    } else {
        ImportStatus::Succeeded
    };

    (summary, status)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ok_movie() -> ImportOutcome {
        ImportOutcome::Movie {
            title: "Movie".into(),
            year: "2024".into(),
            size: 1_500_000_000,
            cost: Duration::from_secs(8),
            has_failed: false,
        }
    }

    fn failed_movie() -> ImportOutcome {
        ImportOutcome::Movie {
            title: "BrokenMovie".into(),
            year: "2024".into(),
            size: 0,
            cost: Duration::from_secs(1),
            has_failed: true,
        }
    }

    fn tv(episodes: Vec<u32>, failed: Vec<u32>) -> ImportOutcome {
        let has_failed = !failed.is_empty();
        ImportOutcome::Tv {
            name: "Show".into(),
            year: "2025".into(),
            season: 1,
            episodes: episodes.clone(),
            missing_episodes: vec![],
            failed_episodes: failed,
            max_episode_number: *episodes.iter().max().unwrap_or(&0),
            number_of_episodes: 10,
            total_size: 6_000_000_000,
            cost: Duration::from_secs(30),
            has_failed,
        }
    }

    #[test]
    fn no_outcomes_is_skipped_status() {
        let (summary, status) = summarize(&[]);
        assert_eq!(status, ImportStatus::Skipped);
        assert!(summary.items.is_empty());
    }

    #[test]
    fn only_skipped_items_yield_skipped_status() {
        let (summary, status) = summarize(&[ImportOutcome::Skipped {
            files: vec!["a.mkv".into(), "b.mkv".into()],
        }]);
        assert_eq!(status, ImportStatus::Skipped);
        assert_eq!(summary.skipped_files, vec!["a.mkv", "b.mkv"]);
    }

    #[test]
    fn all_successful_yields_succeeded() {
        let (summary, status) = summarize(&[ok_movie(), tv(vec![1, 2, 3], vec![])]);
        assert_eq!(status, ImportStatus::Succeeded);
        assert_eq!(summary.total_size, 1_500_000_000 + 6_000_000_000);
        assert_eq!(summary.total_cost_ms, 8_000 + 30_000);
    }

    #[test]
    fn any_failure_with_any_success_yields_partially_failed() {
        let (_, status) = summarize(&[ok_movie(), failed_movie()]);
        assert_eq!(status, ImportStatus::PartiallyFailed);
    }

    #[test]
    fn partial_episode_failure_yields_partially_failed() {
        let (_, status) = summarize(&[tv(vec![1, 2], vec![3, 4])]);
        assert_eq!(status, ImportStatus::PartiallyFailed);
    }

    #[test]
    fn all_failures_without_any_success_yields_failed() {
        let (_, status) = summarize(&[failed_movie(), tv(vec![], vec![1, 2])]);
        assert_eq!(status, ImportStatus::Failed);
    }

    #[test]
    fn tv_episodes_combine_success_and_failure_sorted_with_outcome_flag() {
        let (summary, _) = summarize(&[tv(vec![3, 1], vec![2])]);
        let SummaryItem::Tv { episodes, .. } = &summary.items[0] else {
            panic!("expected tv summary item");
        };
        assert_eq!(
            episodes,
            &vec![
                EpisodeOutcome {
                    episode: 1,
                    succeeded: true,
                },
                EpisodeOutcome {
                    episode: 2,
                    succeeded: false,
                },
                EpisodeOutcome {
                    episode: 3,
                    succeeded: true,
                },
            ]
        );
    }

    #[test]
    fn skipped_files_are_appended_into_summary_in_order() {
        let (summary, status) = summarize(&[
            ImportOutcome::Skipped {
                files: vec!["one.mkv".into()],
            },
            ImportOutcome::Skipped {
                files: vec!["two.mkv".into(), "three.mkv".into()],
            },
        ]);
        assert_eq!(status, ImportStatus::Skipped);
        assert_eq!(
            summary.skipped_files,
            vec!["one.mkv", "two.mkv", "three.mkv"]
        );
    }

    #[test]
    fn skipped_alongside_successful_keeps_successful_status() {
        let (summary, status) = summarize(&[
            ok_movie(),
            ImportOutcome::Skipped {
                files: vec!["unmatched.mkv".into()],
            },
        ]);
        assert_eq!(status, ImportStatus::Succeeded);
        assert_eq!(summary.skipped_files, vec!["unmatched.mkv"]);
    }

    #[test]
    fn source_kind_round_trips_through_string() {
        for kind in [
            ImportSourceKind::Pan123,
            ImportSourceKind::Pan189,
            ImportSourceKind::Pan115,
            ImportSourceKind::Telegram,
            ImportSourceKind::FileIndex,
            ImportSourceKind::Other,
        ] {
            assert_eq!(ImportSourceKind::from_str(kind.as_str()), kind);
        }
    }

    #[test]
    fn unknown_source_kind_string_maps_to_other() {
        assert_eq!(ImportSourceKind::from_str("wat"), ImportSourceKind::Other);
    }

    #[test]
    fn display_fields_use_first_movie_or_tv_item() {
        let (movie, _) = summarize(&[ok_movie()]);
        assert_eq!(
            movie.display_fields(),
            (
                Some("Movie".into()),
                Some("2024".into()),
                Some(1_500_000_000)
            )
        );

        let (tv_summary, _) = summarize(&[tv(vec![1], vec![])]);
        assert_eq!(
            tv_summary.display_fields(),
            (
                Some("Show".into()),
                Some("2025".into()),
                Some(6_000_000_000)
            )
        );

        let (skipped, _) = summarize(&[ImportOutcome::Skipped {
            files: vec!["a.mkv".into()],
        }]);
        assert_eq!(skipped.display_fields(), (None, None, None));
    }

    #[test]
    fn summary_serializes_to_round_trippable_json() {
        let (summary, _) = summarize(&[ok_movie(), tv(vec![1, 2], vec![3])]);
        let json = serde_json::to_string(&summary).unwrap();
        let back: RecordSummary = serde_json::from_str(&json).unwrap();
        assert_eq!(back, summary);
    }
}
