use crate::application::import::ImportedMedia;

fn format_episodes(episodes: &[u32]) -> String {
    if episodes.is_empty() {
        return String::new();
    }

    let mut sorted_episodes = episodes.to_vec();
    sorted_episodes.sort_unstable();
    sorted_episodes.dedup();

    let mut parts = Vec::new();
    let mut i = 0;
    while i < sorted_episodes.len() {
        let start = sorted_episodes[i];
        let mut end = start;
        let mut j = i + 1;
        while j < sorted_episodes.len() && sorted_episodes[j] == end + 1 {
            end = sorted_episodes[j];
            j += 1;
        }

        if start == end {
            parts.push(format!("E{:02}", start));
        } else {
            parts.push(format!("E{:02}-E{:02}", start, end));
        }
        i = j;
    }

    parts.join(",")
}

pub(crate) fn format_imported_media(media: &ImportedMedia) -> Option<String> {
    match media {
        ImportedMedia::Movie {
            title,
            year,
            size,
            cost,
            has_failed,
        } => {
            if *has_failed {
                return None;
            }

            let size_gb = *size as f64 / 1024.0 / 1024.0 / 1024.0;
            Some(format!(
                "🎬 电影 {} ({}) 已入库\n\
                     📊 大小: {:.2} GB\n\
                     ⏱️ 耗时: {:.2} 秒",
                title,
                year,
                size_gb,
                cost.as_secs_f64(),
            ))
        }
        ImportedMedia::Tv {
            name,
            year,
            season,
            episodes,
            missing_episodes,
            max_episode_number,
            total_size,
            number_of_episodes,
            cost,
            has_failed,
        } => {
            if episodes.is_empty() {
                return None;
            }

            let total_size_gb = *total_size as f64 / 1024.0 / 1024.0 / 1024.0;
            let missing_str = if missing_episodes.is_empty() {
                String::new()
            } else {
                format!("🎬️ 缺失集: {}\n", format_episodes(missing_episodes))
            };
            let failure_notice = if *has_failed {
                "⚠️ 部分文件入库失败\n"
            } else {
                ""
            };

            Some(format!(
                "📺 剧集 {} ({}) S{:02} {} 已入库\n{}{}\
                     📦 平均大小: {:.2} GB\n\
                     📊 总大小: {:.2} GB\n\
                     ⏱️ 耗时: {:.2} 秒\n\
                     📦 集数: {}/{}",
                name,
                year,
                season,
                format_episodes(episodes),
                missing_str,
                failure_notice,
                total_size_gb / (episodes.len() as f64),
                total_size_gb,
                cost.as_secs_f64(),
                max_episode_number,
                number_of_episodes,
            ))
        }
    }
}

pub(crate) fn format_import_summaries(imported: &[ImportedMedia]) -> Vec<String> {
    imported.iter().filter_map(format_imported_media).collect()
}

pub(crate) fn format_verbose_import_notes(imported: &[ImportedMedia]) -> Vec<String> {
    let mut notes = Vec::new();

    for media in imported {
        match media {
            ImportedMedia::Movie {
                title,
                year,
                has_failed,
                ..
            } if *has_failed => {
                notes.push(format!(
                    "详细信息: 电影 {} ({}) 已识别，但入库未成功完成。",
                    title, year
                ));
            }
            ImportedMedia::Tv {
                name,
                year,
                season,
                episodes,
                missing_episodes,
                max_episode_number,
                has_failed,
                ..
            } if episodes.is_empty() => {
                let reason = if *has_failed {
                    "已识别该季资源，但没有成功入库任何新分集。"
                } else {
                    "已识别该季资源，但没有新分集入库，通常表示候选分集都因库中已有同集且文件不更小而被跳过。"
                };

                let missing = if missing_episodes.is_empty() {
                    "当前没有检测到缺失集。".to_owned()
                } else {
                    format!("当前缺失集: {}。", format_episodes(missing_episodes))
                };

                notes.push(format!(
                    "详细信息: 剧集 {} ({}) S{:02} {} 当前库内最高集数到 E{:02}。",
                    name, year, season, reason, max_episode_number
                ));
                notes.push(format!("详细信息: {missing}"));
            }
            _ => {}
        }
    }

    notes
}

pub(crate) const NO_NEW_MEDIA_MESSAGE: &str = "没有新入库的媒体";

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    #[test]
    fn test_format_episodes_empty() {
        assert_eq!(format_episodes(&[]), "");
    }

    #[test]
    fn test_format_episodes_single() {
        assert_eq!(format_episodes(&[1]), "E01");
    }

    #[test]
    fn test_format_episodes_sequential() {
        assert_eq!(format_episodes(&[1, 2, 3]), "E01-E03");
    }

    #[test]
    fn test_format_episodes_non_sequential() {
        assert_eq!(format_episodes(&[1, 3, 5]), "E01,E03,E05");
    }

    #[test]
    fn test_format_episodes_mixed() {
        assert_eq!(format_episodes(&[1, 2, 4, 5, 7]), "E01-E02,E04-E05,E07");
    }

    #[test]
    fn test_format_episodes_duplicates() {
        assert_eq!(format_episodes(&[1, 2, 2, 3]), "E01-E03");
    }

    #[test]
    fn test_format_episodes_unsorted() {
        assert_eq!(format_episodes(&[3, 1, 2]), "E01-E03");
    }

    #[test]
    fn test_format_episodes_zero() {
        assert_eq!(format_episodes(&[0]), "E00");
    }

    #[test]
    fn test_format_episodes_large_numbers() {
        assert_eq!(format_episodes(&[99, 100, 102]), "E99-E100,E102");
    }

    #[test]
    fn format_movie_summary_for_cli() {
        let summary = format_imported_media(&ImportedMedia::Movie {
            title: "Movie".into(),
            year: "2024".into(),
            size: 3 * 1024 * 1024 * 1024,
            cost: Duration::from_secs(12),
            has_failed: false,
        })
        .unwrap();

        assert!(summary.contains("电影 Movie (2024) 已入库"));
        assert!(summary.contains("大小: 3.00 GB"));
        assert!(summary.contains("耗时: 12.00 秒"));
    }

    #[test]
    fn format_tv_summary_for_cli() {
        let summary = format_imported_media(&ImportedMedia::Tv {
            name: "Show".into(),
            year: "2025".into(),
            season: 1,
            episodes: vec![1, 2, 3],
            missing_episodes: vec![4],
            max_episode_number: 3,
            total_size: 6 * 1024 * 1024 * 1024,
            number_of_episodes: 4,
            cost: Duration::from_secs(30),
            has_failed: true,
        })
        .unwrap();

        assert!(summary.contains("剧集 Show (2025) S01 E01-E03 已入库"));
        assert!(summary.contains("缺失集: E04"));
        assert!(summary.contains("部分文件入库失败"));
        assert!(summary.contains("总大小: 6.00 GB"));
        assert!(summary.contains("集数: 3/4"));
    }

    #[test]
    fn format_import_summaries_returns_empty_when_all_entries_are_skipped() {
        let summaries = format_import_summaries(&[ImportedMedia::Movie {
            title: "Movie".into(),
            year: "2024".into(),
            size: 1,
            cost: Duration::from_secs(1),
            has_failed: true,
        }]);

        assert!(summaries.is_empty());
        assert_eq!(NO_NEW_MEDIA_MESSAGE, "没有新入库的媒体");
    }

    #[test]
    fn format_verbose_import_notes_explains_skipped_tv() {
        let notes = format_verbose_import_notes(&[ImportedMedia::Tv {
            name: "Show".into(),
            year: "2025".into(),
            season: 1,
            episodes: vec![],
            missing_episodes: vec![2, 5],
            max_episode_number: 6,
            total_size: 0,
            number_of_episodes: 6,
            cost: Duration::from_secs(30),
            has_failed: false,
        }]);

        assert!(
            notes
                .iter()
                .any(|note| note.contains("没有新分集入库") && note.contains("E06"))
        );
        assert!(notes.iter().any(|note| note.contains("E02,E05")));
    }
}
