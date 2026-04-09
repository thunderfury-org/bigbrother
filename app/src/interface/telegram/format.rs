use crate::application::import::ImportedMedia;

fn format_episodes(episodes: &[u32]) -> String {
    if episodes.is_empty() {
        return String::new();
    }

    let mut sorted_episodes = episodes.to_vec();
    sorted_episodes.sort_unstable();
    sorted_episodes.dedup(); // Remove duplicates

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

pub(super) fn format_imported_media(media: &ImportedMedia) -> Option<String> {
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
            ..
        } => {
            if episodes.is_empty() {
                return None;
            }

            let total_size_gb = *total_size as f64 / 1024.0 / 1024.0 / 1024.0;
            let missing_str = if missing_episodes.is_empty() {
                "".to_owned()
            } else {
                format!("🎬️ 缺失集: {}\n", format_episodes(missing_episodes))
            };

            Some(format!(
                "📺 剧集 {} ({}) S{:02} {} 已入库\n{}\
                     📦 平均大小: {:.2} GB\n\
                     📊 总大小: {:.2} GB\n\
                     ⏱️ 耗时: {:.2} 秒\n\
                     📦 集数: {}/{}",
                name,
                year,
                season,
                format_episodes(episodes),
                missing_str,
                total_size_gb / (episodes.len() as f64),
                total_size_gb,
                cost.as_secs_f64(),
                max_episode_number,
                number_of_episodes,
            ))
        }
    }
}

#[cfg(test)]
mod tests {
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
}
