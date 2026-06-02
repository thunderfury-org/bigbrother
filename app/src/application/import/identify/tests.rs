use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use super::*;
use crate::application::import_ports::{MetadataCatalog, TitleExtractor};
use crate::domain::import::inner::MediaFile;
use crate::domain::import::{
    Genre, MovieDetail, SearchMovieResult, SearchTvResult, Season, TvDetail,
};
use crate::domain::media::{FileType, MediaKind, Metadata, Title};
use crate::domain::share::{FileHash, RawFile};

type SearchKey = (String, String);

#[derive(Clone, Default)]
struct FakeCatalog {
    movie_details: Arc<Mutex<HashMap<u32, MovieDetail>>>,
    tv_details: Arc<Mutex<HashMap<u32, TvDetail>>>,
    movie_search_results: Arc<Mutex<HashMap<SearchKey, Vec<SearchMovieResult>>>>,
    tv_search_results: Arc<Mutex<HashMap<SearchKey, Vec<SearchTvResult>>>>,
}

impl MetadataCatalog for FakeCatalog {
    async fn search_movie(&self, title: &str, year: &str) -> AppResult<Vec<SearchMovieResult>> {
        Ok(self
            .movie_search_results
            .lock()
            .unwrap()
            .get(&(title.to_string(), year.to_string()))
            .cloned()
            .unwrap_or_default())
    }

    async fn get_movie_detail(&self, id: u32) -> AppResult<Option<MovieDetail>> {
        Ok(self.movie_details.lock().unwrap().get(&id).cloned())
    }

    async fn search_tv(&self, title: &str, year: &str) -> AppResult<Vec<SearchTvResult>> {
        Ok(self
            .tv_search_results
            .lock()
            .unwrap()
            .get(&(title.to_string(), year.to_string()))
            .cloned()
            .unwrap_or_default())
    }

    async fn get_tv_detail(&self, id: u32) -> AppResult<Option<TvDetail>> {
        Ok(self.tv_details.lock().unwrap().get(&id).cloned())
    }
}

#[derive(Clone)]
struct NoOpTitleExtractor;

impl TitleExtractor for NoOpTitleExtractor {
    async fn extract_title(&self, _description: &str) -> AppResult<Option<Title>> {
        Ok(None)
    }
}

fn raw_file(name: &str) -> RawFile {
    RawFile {
        id: None,
        name: name.to_string(),
        path: format!("/test/{name}"),
        hash: FileHash::Md5("test".into()),
        size: 1024,
    }
}

fn movie_media_file(name: &str, title: &str, year: &str) -> MediaFile {
    MediaFile {
        metadata: Box::new(Metadata {
            media_kind: MediaKind::Movie,
            file_type: FileType::Video,
            titles: vec![Title {
                title: title.to_string(),
                language: "en".to_string(),
            }],
            year: year.to_string(),
            ..Default::default()
        }),
        video: raw_file(name),
        subtitles: Vec::new(),
        descriptions: Vec::new(),
    }
}

fn tv_media_file(name: &str, title: &str, year: &str, season: u32, episode: u32) -> MediaFile {
    MediaFile {
        metadata: Box::new(Metadata {
            media_kind: MediaKind::TvEpisode,
            file_type: FileType::Video,
            titles: vec![Title {
                title: title.to_string(),
                language: "en".to_string(),
            }],
            year: year.to_string(),
            season_number: Some(season),
            episode_number: Some(episode),
            ..Default::default()
        }),
        video: raw_file(name),
        subtitles: Vec::new(),
        descriptions: Vec::new(),
    }
}

fn tv_detail(id: u32, name: &str, seasons: u32) -> TvDetail {
    TvDetail {
        id,
        name: name.into(),
        first_air_date: "2008-01-20".into(),
        number_of_episodes: 62,
        number_of_seasons: seasons,
        origin_country: vec!["US".into()],
        original_language: "en".into(),
        original_name: name.into(),
        genres: vec![Genre {
            id: 1,
            name: "Drama".into(),
        }],
        seasons: (1..=seasons)
            .map(|n| Season {
                id: n,
                name: format!("Season {n}"),
                episode_count: 10,
                season_number: n,
            })
            .collect(),
    }
}

fn movie_detail(id: u32, title: &str, year: &str) -> MovieDetail {
    MovieDetail {
        id,
        title: title.into(),
        adult: false,
        genres: Vec::new(),
        original_language: "en".into(),
        original_title: title.into(),
        origin_country: Vec::new(),
        release_date: format!("{year}-01-01"),
    }
}

#[tokio::test]
async fn identify_buckets_movie_into_media_movie() {
    let catalog = FakeCatalog::default();
    catalog.movie_search_results.lock().unwrap().insert(
        ("Inception".into(), "2010".into()),
        vec![SearchMovieResult {
            id: 27205,
            title: "Inception".into(),
            original_title: "Inception".into(),
        }],
    );
    catalog
        .movie_details
        .lock()
        .unwrap()
        .insert(27205, movie_detail(27205, "Inception", "2010"));
    let mut svc = MediaIdentifyService::new(catalog, NoOpTitleExtractor);
    let files = vec![movie_media_file("Inception.2010.mkv", "Inception", "2010")];

    let outcome = svc.identify(&files).await.unwrap();

    assert_eq!(outcome.unmatched.len(), 0);
    assert_eq!(outcome.groups.len(), 1);
    match &outcome.groups[0] {
        Media::Movie { detail, files } => {
            assert_eq!(detail.id, 27205);
            assert_eq!(files.len(), 1);
            assert_eq!(files[0].video.name, "Inception.2010.mkv");
        }
        Media::Tv { .. } => panic!("expected Media::Movie, got Media::Tv"),
    }
}

#[tokio::test]
async fn identify_groups_tv_files_by_tmdb_id() {
    let catalog = FakeCatalog::default();
    catalog.tv_search_results.lock().unwrap().insert(
        ("Breaking Bad".into(), "2008".into()),
        vec![SearchTvResult {
            id: 1396,
            name: "Breaking Bad".into(),
            original_name: "Breaking Bad".into(),
        }],
    );
    catalog
        .tv_details
        .lock()
        .unwrap()
        .insert(1396, tv_detail(1396, "Breaking Bad", 5));
    let mut svc = MediaIdentifyService::new(catalog, NoOpTitleExtractor);
    let files = vec![
        tv_media_file("Breaking.Bad.S01E05.mkv", "Breaking Bad", "2008", 1, 5),
        tv_media_file("Breaking.Bad.S01E06.mkv", "Breaking Bad", "2008", 1, 6),
    ];

    let outcome = svc.identify(&files).await.unwrap();

    assert_eq!(outcome.unmatched.len(), 0);
    assert_eq!(outcome.groups.len(), 1);
    match &outcome.groups[0] {
        Media::Tv { detail, files } => {
            assert_eq!(detail.id, 1396);
            assert_eq!(detail.name, "Breaking Bad");
            assert_eq!(files.len(), 1);
            let season_1 = files.get(&1).expect("expected season 1");
            assert_eq!(season_1.len(), 2);
            assert_eq!(
                season_1.get(&5).unwrap()[0].video.name,
                "Breaking.Bad.S01E05.mkv"
            );
            assert_eq!(
                season_1.get(&6).unwrap()[0].video.name,
                "Breaking.Bad.S01E06.mkv"
            );
        }
        Media::Movie { .. } => panic!("expected Media::Tv, got Media::Movie"),
    }
}

#[tokio::test]
async fn identify_returns_unmatched_when_tmdb_returns_none() {
    let catalog = FakeCatalog::default();
    let mut svc = MediaIdentifyService::new(catalog, NoOpTitleExtractor);
    let files = vec![movie_media_file("Unknown.2020.mkv", "Unknown", "2020")];

    let outcome = svc.identify(&files).await.unwrap();

    assert_eq!(outcome.groups.len(), 0);
    assert_eq!(outcome.unmatched.len(), 1);
    assert_eq!(outcome.unmatched[0].file_name, "Unknown.2020.mkv");
    assert_eq!(outcome.unmatched[0].file_path, "/test/Unknown.2020.mkv");
}

#[tokio::test]
async fn identify_returns_unmatched_when_episode_slot_unresolved() {
    let catalog = FakeCatalog::default();
    catalog.tv_search_results.lock().unwrap().insert(
        ("Breaking Bad".into(), "2008".into()),
        vec![SearchTvResult {
            id: 1396,
            name: "Breaking Bad".into(),
            original_name: "Breaking Bad".into(),
        }],
    );
    catalog
        .tv_details
        .lock()
        .unwrap()
        .insert(1396, tv_detail(1396, "Breaking Bad", 5));
    let mut svc = MediaIdentifyService::new(catalog, NoOpTitleExtractor);
    // Multi-season show with no episode number → resolve_tv_episode_slot returns None
    let bad_file = MediaFile {
        metadata: Box::new(Metadata {
            media_kind: MediaKind::TvEpisode,
            file_type: FileType::Video,
            titles: vec![Title {
                title: "Breaking Bad".to_string(),
                language: "en".to_string(),
            }],
            year: "2008".to_string(),
            season_number: Some(1),
            episode_number: None,
            ..Default::default()
        }),
        video: raw_file("Breaking.Bad.Special.mkv"),
        subtitles: Vec::new(),
        descriptions: Vec::new(),
    };
    let files = vec![bad_file];

    let outcome = svc.identify(&files).await.unwrap();

    assert_eq!(outcome.groups.len(), 0);
    assert_eq!(outcome.unmatched.len(), 1);
    assert_eq!(outcome.unmatched[0].file_name, "Breaking.Bad.Special.mkv");
}
