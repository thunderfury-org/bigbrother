use std::collections::HashMap;

use tracing::info;

use super::category;
use crate::{
    client::tmdb::{MovieDetail, TvDetail},
    error::AppResult,
    media::Metadata,
    state::AppState,
};

mod group;
mod library;
mod metadata;
pub(super) mod share;
mod tmdb_info;
mod transfer;

#[derive(Debug)]
struct RawFile {
    pub id: i64,
    pub name: String,
    pub etag: String,
    pub size: u64,
    pub path: String,
}

struct MediaFile {
    metadata: Metadata,
    raw: RawFile,
}

enum Media<'a> {
    Movie {
        detail: MovieDetail,
        files: Vec<&'a MediaFile>,
    },
    Tv {
        detail: TvDetail,
        // (season, episode) -> files[]
        files: HashMap<u32, HashMap<u32, Vec<&'a MediaFile>>>,
    },
}

#[derive(Debug, Default, Clone)]
pub struct ImportSummary {
    pub success: u32,
    pub failed: u32,
    pub skipped: u32,
    pub total: u32,
    pub total_size: u64,
    pub cost: std::time::Duration,
    pub unknown_files: Vec<String>,
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
