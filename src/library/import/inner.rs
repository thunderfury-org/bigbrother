use std::collections::HashMap;

use crate::{
    client::tmdb::{MovieDetail, TvDetail},
    media::Metadata,
};

#[derive(Debug, Clone)]
pub(super) struct RawFile {
    pub id: Option<i64>,
    pub name: String,
    pub etag: String,
    pub size: u64,
    pub path: String,
}

pub(super) struct MediaFile {
    pub metadata: Box<Metadata>,
    pub video: Box<RawFile>,
    pub subtitles: Vec<Box<RawFile>>,
}

impl MediaFile {
    pub fn file_count(&self) -> usize {
        1 + self.subtitles.len()
    }
}

pub(super) enum Media<'a> {
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
