use std::collections::HashMap;

use super::category;
use crate::{
    client::tmdb::{MovieDetail, TvDetail},
    state::AppState,
};

mod group;
mod inner;
mod library;
mod metadata;
pub(super) mod share;
mod tmdb_info;
mod transfer;

#[derive(Debug, Default, Clone)]
pub struct ImportSummary {
    pub success: usize,
    pub failed: usize,
    pub skipped: usize,
    pub total: usize,
    pub total_size: u64,
    pub cost: std::time::Duration,
}

pub(super) struct Importer {
    state: AppState,
    tv_info_cache: HashMap<String, Option<TvDetail>>,
    movie_info_cache: HashMap<String, Option<MovieDetail>>,
    summary: ImportSummary,
    start_time: std::time::Instant,
}

impl Importer {
    pub fn new(state: AppState) -> Self {
        Self {
            state,
            tv_info_cache: HashMap::new(),
            movie_info_cache: HashMap::new(),
            summary: ImportSummary::default(),
            start_time: std::time::Instant::now(),
        }
    }
}
