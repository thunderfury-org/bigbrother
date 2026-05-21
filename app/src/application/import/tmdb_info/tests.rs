use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use super::*;
use crate::domain::import::{SearchMovieResult, SearchTvResult, Season};
use crate::domain::media::{Metadata, Title};

type TvSearchResults = HashMap<(String, String), Vec<SearchTvResult>>;

#[derive(Clone, Default)]
struct FakeMetadataCatalog {
    movie_detail_calls: Arc<Mutex<Vec<u32>>>,
    tv_detail_calls: Arc<Mutex<Vec<u32>>>,
    movie_search_calls: Arc<Mutex<Vec<(String, String)>>>,
    tv_search_calls: Arc<Mutex<Vec<(String, String)>>>,
    movie_search_results: Arc<Mutex<Vec<SearchMovieResult>>>,
    tv_search_results: Arc<Mutex<TvSearchResults>>,
    movie_details: Arc<Mutex<HashMap<u32, MovieDetail>>>,
    tv_details: Arc<Mutex<HashMap<u32, TvDetail>>>,
}

impl MetadataCatalog for FakeMetadataCatalog {
    async fn search_movie(&self, title: &str, year: &str) -> AppResult<Vec<SearchMovieResult>> {
        self.movie_search_calls
            .lock()
            .unwrap()
            .push((title.to_string(), year.to_string()));
        let configured = self.movie_search_results.lock().unwrap();
        if !configured.is_empty() {
            return Ok(configured.clone());
        }
        Ok(vec![SearchMovieResult {
            id: 7,
            title: title.to_string(),
            original_title: title.to_string(),
        }])
    }

    async fn get_movie_detail(&self, id: u32) -> AppResult<Option<MovieDetail>> {
        self.movie_detail_calls.lock().unwrap().push(id);
        if let Some(detail) = self.movie_details.lock().unwrap().get(&id).cloned() {
            return Ok(Some(detail));
        }
        Ok(Some(MovieDetail {
            id,
            title: "Movie".into(),
            adult: false,
            genres: Vec::new(),
            original_language: "en".into(),
            original_title: "Movie".into(),
            origin_country: Vec::new(),
            release_date: "2024-01-01".into(),
        }))
    }

    async fn search_tv(&self, title: &str, year: &str) -> AppResult<Vec<SearchTvResult>> {
        self.tv_search_calls
            .lock()
            .unwrap()
            .push((title.to_string(), year.to_string()));
        Ok(self
            .tv_search_results
            .lock()
            .unwrap()
            .get(&(title.to_string(), year.to_string()))
            .cloned()
            .unwrap_or_default())
    }

    async fn get_tv_detail(&self, id: u32) -> AppResult<Option<TvDetail>> {
        self.tv_detail_calls.lock().unwrap().push(id);
        Ok(self.tv_details.lock().unwrap().get(&id).cloned())
    }
}

#[tokio::test]
async fn get_movie_info_skips_invalid_tmdb_id_without_panicking() {
    let catalog = FakeMetadataCatalog::default();
    let mut lookup = TmdbLookup::new(catalog.clone());
    let meta = Metadata {
        tmdb_id: "not-a-number".into(),
        ..Default::default()
    };

    let movie = lookup.get_movie_info(&meta).await.unwrap();

    assert!(movie.is_none());
    assert!(catalog.movie_detail_calls.lock().unwrap().is_empty());
}

#[tokio::test]
async fn get_tv_info_skips_invalid_tmdb_id_without_panicking() {
    let catalog = FakeMetadataCatalog::default();
    let mut lookup = TmdbLookup::new(catalog.clone());
    let meta = Metadata {
        tmdb_id: "bad-tv-id".into(),
        ..Default::default()
    };

    let tv = lookup.get_tv_info(&meta).await.unwrap();

    assert!(tv.is_none());
    assert!(catalog.tv_detail_calls.lock().unwrap().is_empty());
}

#[tokio::test]
async fn get_movie_info_falls_back_to_title_search_after_invalid_tmdb_id() {
    let catalog = FakeMetadataCatalog::default();
    let mut lookup = TmdbLookup::new(catalog.clone());
    let meta = Metadata {
        tmdb_id: "oops".into(),
        titles: vec![Title {
            title: "Movie".into(),
            language: "en".into(),
        }],
        year: "2024".into(),
        ..Default::default()
    };

    let movie = lookup.get_movie_info(&meta).await.unwrap();

    assert_eq!(movie.map(|item| item.id), Some(7));
    assert_eq!(
        catalog.movie_search_calls.lock().unwrap().as_slice(),
        &[("Movie".to_string(), "2024".to_string())]
    );
    assert_eq!(catalog.movie_detail_calls.lock().unwrap().as_slice(), &[7]);
}

#[tokio::test]
async fn get_movie_info_matches_normalized_title_and_year_from_multiple_candidates() {
    let catalog = FakeMetadataCatalog::default();
    catalog.movie_search_results.lock().unwrap().extend([
        SearchMovieResult {
            id: 11,
            title: "The Lord of the Rings: The Two Towers".into(),
            original_title: "The Lord of the Rings: The Two Towers".into(),
        },
        SearchMovieResult {
            id: 12,
            title: "The Lord of the Rings: The Two Towers".into(),
            original_title: "The Lord of the Rings: The Two Towers".into(),
        },
    ]);
    catalog.movie_details.lock().unwrap().extend([
        (
            11,
            MovieDetail {
                id: 11,
                title: "The Lord of the Rings: The Two Towers".into(),
                adult: false,
                genres: Vec::new(),
                original_language: "en".into(),
                original_title: "The Lord of the Rings: The Two Towers".into(),
                origin_country: Vec::new(),
                release_date: "2001-12-19".into(),
            },
        ),
        (
            12,
            MovieDetail {
                id: 12,
                title: "The Lord of the Rings: The Two Towers".into(),
                adult: false,
                genres: Vec::new(),
                original_language: "en".into(),
                original_title: "The Lord of the Rings: The Two Towers".into(),
                origin_country: Vec::new(),
                release_date: "2002-12-18".into(),
            },
        ),
    ]);

    let mut lookup = TmdbLookup::new(catalog.clone());
    let meta = Metadata {
        titles: vec![Title {
            title: "The Lord of the Rings： The Two Towers".into(),
            language: "en".into(),
        }],
        year: "2002".into(),
        ..Default::default()
    };

    let movie = lookup.get_movie_info(&meta).await.unwrap();

    assert_eq!(movie.map(|item| item.id), Some(12));
}

#[tokio::test]
async fn get_tv_info_tries_later_titles_after_cached_none_for_earlier_title() {
    let catalog = FakeMetadataCatalog::default();
    catalog.tv_search_results.lock().unwrap().insert(
        ("21世纪大君夫人".to_string(), "2026".to_string()),
        vec![SearchTvResult {
            id: 278573,
            name: "21世纪大君夫人".into(),
            original_name: "21세기 대군부인".into(),
        }],
    );
    catalog.tv_details.lock().unwrap().insert(
        278573,
        TvDetail {
            id: 278573,
            name: "21世纪大君夫人".into(),
            first_air_date: "2026-04-10".into(),
            number_of_episodes: 12,
            number_of_seasons: 1,
            origin_country: vec!["KR".into()],
            original_language: "ko".into(),
            original_name: "21세기 대군부인".into(),
            genres: Vec::new(),
            seasons: vec![Season {
                id: 1,
                name: "Season 1".into(),
                episode_count: 12,
                season_number: 1,
            }],
        },
    );

    let mut lookup = TmdbLookup::new(catalog.clone());
    let meta = Metadata {
        titles: vec![
            Title {
                title: "Perfect Crown DSNP".into(),
                language: "en".into(),
            },
            Title {
                title: "21世纪大君夫人".into(),
                language: "zh".into(),
            },
        ],
        year: "2026".into(),
        ..Default::default()
    };

    let first = lookup.get_tv_info(&meta).await.unwrap();
    let second = lookup.get_tv_info(&meta).await.unwrap();

    assert_eq!(first.as_ref().map(|item| item.id), Some(278573));
    assert_eq!(second.as_ref().map(|item| item.id), Some(278573));
    assert_eq!(
        catalog.tv_search_calls.lock().unwrap().as_slice(),
        &[
            ("Perfect Crown DSNP".to_string(), "2026".to_string()),
            ("21世纪大君夫人".to_string(), "2026".to_string()),
        ]
    );
}
