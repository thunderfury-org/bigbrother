use super::super::import_tests::FakeTitleExtractor;
use super::*;
use crate::application::ports::MetadataCatalog;
use crate::domain::import::{MovieDetail, SearchMovieResult, SearchTvResult, TvDetail};
use crate::domain::share::{FileHash, RawFile};
use crate::error::AppResult;

#[derive(Clone, Default)]
struct StubMetadataCatalog;

impl MetadataCatalog for StubMetadataCatalog {
    async fn search_movie(&self, title: &str, _year: &str) -> AppResult<Vec<SearchMovieResult>> {
        Ok(vec![SearchMovieResult {
            id: 100,
            title: title.to_string(),
            original_title: title.to_string(),
        }])
    }

    async fn get_movie_detail(&self, id: u32) -> AppResult<Option<MovieDetail>> {
        Ok(Some(MovieDetail {
            id,
            title: "测试电影".into(),
            adult: false,
            genres: Vec::new(),
            original_language: "zh".into(),
            original_title: "Test Movie".into(),
            origin_country: vec!["CN".into()],
            release_date: "2024-06-15".into(),
        }))
    }

    async fn search_tv(&self, title: &str, _year: &str) -> AppResult<Vec<SearchTvResult>> {
        Ok(vec![SearchTvResult {
            id: 200,
            name: title.to_string(),
            original_name: title.to_string(),
        }])
    }

    async fn get_tv_detail(&self, id: u32) -> AppResult<Option<TvDetail>> {
        Ok(Some(TvDetail {
            id,
            name: "测试剧集".into(),
            first_air_date: "2025-01-01".into(),
            number_of_episodes: 12,
            number_of_seasons: 1,
            origin_country: vec!["CN".into()],
            original_language: "zh".into(),
            original_name: "Test Show".into(),
            genres: Vec::new(),
            seasons: Vec::new(),
        }))
    }
}

fn raw_file(name: &str, path: &str) -> RawFile {
    RawFile {
        id: None,
        name: name.to_string(),
        hash: FileHash::Md5(String::new()),
        size: 1024,
        path: path.to_string(),
    }
}

#[tokio::test]
async fn parse_movie_with_tmdb_match() {
    let service = ParseService::new(StubMetadataCatalog, FakeTitleExtractor);
    let results = service
        .parse_media_files(
            vec![raw_file("Test.Movie.2024.1080p.mkv", "/share")],
            Vec::new(),
        )
        .await
        .unwrap();

    assert_eq!(results.len(), 1);
    match &results[0] {
        ParsedMediaInfo::Movie {
            tmdb_id,
            tmdb_title,
            resolution,
            ..
        } => {
            assert_eq!(*tmdb_id, Some(100));
            assert_eq!(tmdb_title.as_deref(), Some("测试电影"));
            assert_eq!(resolution, "1080p");
        }
        _ => panic!("expected Movie variant"),
    }
}

#[tokio::test]
async fn parse_tv_episode_with_tmdb_match() {
    let service = ParseService::new(StubMetadataCatalog, FakeTitleExtractor);
    let results = service
        .parse_media_files(
            vec![raw_file("Test.Show.S01E02.720p.mkv", "/share")],
            Vec::new(),
        )
        .await
        .unwrap();

    assert_eq!(results.len(), 1);
    match &results[0] {
        ParsedMediaInfo::Tv {
            tmdb_id,
            season_number,
            episode_number,
            ..
        } => {
            assert_eq!(*tmdb_id, Some(200));
            assert_eq!(*season_number, Some(1));
            assert_eq!(*episode_number, Some(2));
        }
        _ => panic!("expected Tv variant"),
    }
}

#[tokio::test]
async fn parse_unmatched_file() {
    let service = ParseService::new(StubMetadataCatalog, FakeTitleExtractor);
    // A file with no recognizable title should produce Unmatched
    let results = service
        .parse_media_files(vec![raw_file("random_file.txt", "/share")], Vec::new())
        .await
        .unwrap();

    // Non-media files are filtered out by build_media_files
    assert_eq!(results.len(), 0);
}

#[tokio::test]
async fn parse_includes_path_in_output() {
    let service = ParseService::new(StubMetadataCatalog, FakeTitleExtractor);
    let results = service
        .parse_media_files(
            vec![raw_file("Movie.2024.mkv", "/subpath/to/dir")],
            Vec::new(),
        )
        .await
        .unwrap();

    assert_eq!(results.len(), 1);
    match &results[0] {
        ParsedMediaInfo::Movie { path, .. } => {
            assert_eq!(path, "/subpath/to/dir/Movie.2024.mkv");
        }
        _ => panic!("expected Movie variant"),
    }
}
