use std::time::Duration;

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum ImportOutcome {
    Movie {
        title: String,
        year: String,
        size: u64,
        cost: Duration,
        has_failed: bool,
    },
    Tv {
        name: String,
        year: String,
        season: u32,
        episodes: Vec<u32>,
        missing_episodes: Vec<u32>,
        failed_episodes: Vec<u32>,
        max_episode_number: u32,
        number_of_episodes: u32,
        total_size: u64,
        cost: Duration,
        has_failed: bool,
    },
    Skipped {
        files: Vec<String>,
    },
}
