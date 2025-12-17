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
