pub(super) fn format_episodes(episodes: &[u32]) -> String {
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
