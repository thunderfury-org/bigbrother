use std::collections::HashMap;

use crate::{
    client::tmdb::{MovieDetail, TvDetail},
    media::Metadata,
    state::AppState,
};

mod category;
mod group;
mod inner;
pub(super) mod json;
mod library;
mod metadata;
pub(super) mod share;
mod tmdb_info;
mod transfer;

#[derive(Debug, Default, Clone)]
pub struct ImportSummary {
    pub success: usize,
    pub failed: usize,
    pub total_size: u64,
    pub cost: std::time::Duration,
}

pub(super) struct Importer {
    state: AppState,
    tv_info_cache: HashMap<String, Option<TvDetail>>,
    movie_info_cache: HashMap<String, Option<MovieDetail>>,
    metadata_cache: HashMap<String, Box<Metadata>>,
    summary: ImportSummary,
    start_time: std::time::Instant,
}

impl Importer {
    pub fn new(state: AppState) -> Self {
        Self {
            state,
            tv_info_cache: HashMap::new(),
            movie_info_cache: HashMap::new(),
            metadata_cache: HashMap::new(),
            summary: ImportSummary::default(),
            start_time: std::time::Instant::now(),
        }
    }
}

impl Default for Importer {
    fn default() -> Self {
        Self::new(AppState::default())
    }
}
