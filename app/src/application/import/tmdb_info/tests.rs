use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use super::*;
use crate::domain::import::{SearchMovieResult, SearchTvResult};
use crate::domain::media::{Metadata, Title};

#[derive(Clone, Default)]
struct FakeMetadataCatalog {
    movie_detail_calls: Arc<Mutex<Vec<u32>>>,
    tv_detail_calls: Arc<Mutex<Vec<u32>>>,
    movie_search_calls: Arc<Mutex<Vec<(String, String)>>>,
    movie_search_results: Arc<Mutex<Vec<SearchMovieResult>>>,
    movie_details: Arc<Mutex<HashMap<u32, MovieDetail>>>,
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

    async fn search_tv(&self, _title: &str, _year: &str) -> AppResult<Vec<SearchTvResult>> {
        Ok(Vec::new())
    }

    async fn get_tv_detail(&self, id: u32) -> AppResult<Option<TvDetail>> {
        self.tv_detail_calls.lock().unwrap().push(id);
        Ok(None)
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
