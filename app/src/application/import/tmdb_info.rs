use std::collections::HashMap;

use tracing::{info, warn};

use crate::{
    application::import_ports::MetadataCatalog,
    domain::{
        import::{MovieDetail, SearchMovieResult, SearchTvResult, TvDetail},
        media::Metadata,
    },
    error::AppResult,
};

pub(super) struct TmdbLookup<M> {
    metadata_catalog: M,
    tv_info_cache: HashMap<String, Option<TvDetail>>,
    movie_info_cache: HashMap<String, Option<MovieDetail>>,
}

impl<M> TmdbLookup<M>
where
    M: MetadataCatalog,
{
    pub(super) fn new(metadata_catalog: M) -> Self {
        Self {
            metadata_catalog,
            tv_info_cache: HashMap::new(),
            movie_info_cache: HashMap::new(),
        }
    }

    pub(super) async fn get_movie_info(
        &mut self,
        meta: &Metadata,
    ) -> AppResult<Option<MovieDetail>> {
        if let Some(tmdb_id) = parsed_tmdb_id(meta, "movie") {
            let cache_key = format!("movie:{}", meta.tmdb_id);
            if let Some(movie) = self.movie_info_cache.get(&cache_key) {
                return Ok(movie.clone());
            }

            if let Some(movie) = self.metadata_catalog.get_movie_detail(tmdb_id).await? {
                info!(
                    "Movie found for title: {}, year: {}, id: {}",
                    movie.title, movie.release_date, movie.id
                );
                self.movie_info_cache.insert(cache_key, Some(movie.clone()));
                return Ok(Some(movie));
            }
        }

        for title in &meta.titles {
            let cache_key = format!("movie:{}:{}", title.title, meta.year);
            if let Some(movie) = self.movie_info_cache.get(&cache_key) {
                return Ok(movie.clone());
            }
            let movies = self
                .metadata_catalog
                .search_movie(&title.title, &meta.year)
                .await?;
            match resolve_movie_candidate(&title.title, movies, &self.metadata_catalog).await? {
                Some(movie) => {
                    self.movie_info_cache.insert(cache_key, Some(movie.clone()));
                    return Ok(Some(movie));
                }
                None => {
                    self.movie_info_cache.insert(cache_key, None);
                    continue;
                }
            }
        }
        Ok(None)
    }

    pub(super) async fn get_tv_info(&mut self, meta: &Metadata) -> AppResult<Option<TvDetail>> {
        if let Some(tmdb_id) = parsed_tmdb_id(meta, "tv") {
            let cache_key = format!("tv:{}", meta.tmdb_id);
            if let Some(tv) = self.tv_info_cache.get(&cache_key) {
                return Ok(tv.clone());
            }

            if let Some(tv) = self.metadata_catalog.get_tv_detail(tmdb_id).await? {
                info!(
                    "Tv found for title: {}, year: {}, id: {}",
                    tv.name, tv.first_air_date, tv.id
                );

                self.tv_info_cache.insert(cache_key, Some(tv.clone()));
                return Ok(Some(tv));
            }
        }

        for title in &meta.titles {
            let cache_key = format!("tv:{}:{}", title.title, meta.year);
            if let Some(tv) = self.tv_info_cache.get(&cache_key) {
                return Ok(tv.clone());
            }
            let tvs = self
                .metadata_catalog
                .search_tv(&title.title, &meta.year)
                .await?;
            match resolve_tv_candidate(&title.title, tvs, &self.metadata_catalog).await? {
                Some(tv) => {
                    self.tv_info_cache.insert(cache_key, Some(tv.clone()));
                    return Ok(Some(tv));
                }
                None => {
                    self.tv_info_cache.insert(cache_key, None);
                    continue;
                }
            }
        }
        Ok(None)
    }
}

fn parsed_tmdb_id(meta: &Metadata, media_type: &str) -> Option<u32> {
    if meta.tmdb_id.is_empty() {
        return None;
    }

    match meta.tmdb_id.parse() {
        Ok(tmdb_id) => Some(tmdb_id),
        Err(error) => {
            warn!(
                "Invalid {} tmdb id '{}', title candidates: {:?}, error: {}",
                media_type,
                meta.tmdb_id,
                meta.titles
                    .iter()
                    .map(|title| &title.title)
                    .collect::<Vec<_>>(),
                error
            );
            None
        }
    }
}

async fn resolve_movie_candidate<M>(
    title: &str,
    movies: Vec<SearchMovieResult>,
    metadata_catalog: &M,
) -> AppResult<Option<MovieDetail>>
where
    M: MetadataCatalog,
{
    match movies.len() {
        0 => Ok(None),
        1 => {
            info!("Movie found for title: {}, id: {}", title, movies[0].id);
            metadata_catalog.get_movie_detail(movies[0].id).await
        }
        _ => {
            for movie in &movies {
                if movie.original_title == title || movie.title == title {
                    info!("Movie found for title: {}, id: {}", title, movie.id);
                    return metadata_catalog.get_movie_detail(movie.id).await;
                }
            }
            Ok(None)
        }
    }
}

async fn resolve_tv_candidate<M>(
    title: &str,
    tvs: Vec<SearchTvResult>,
    metadata_catalog: &M,
) -> AppResult<Option<TvDetail>>
where
    M: MetadataCatalog,
{
    match tvs.len() {
        0 => Ok(None),
        1 => {
            info!("Tv found for title: {}, id: {}", title, tvs[0].id);
            metadata_catalog.get_tv_detail(tvs[0].id).await
        }
        _ => {
            for tv in &tvs {
                if tv.original_name == title || tv.name == title {
                    info!("Tv found for title: {}, id: {}", title, tv.id);
                    return metadata_catalog.get_tv_detail(tv.id).await;
                }
            }
            Ok(None)
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::*;
    use crate::domain::media::{Metadata, Title};

    #[derive(Clone, Default)]
    struct FakeMetadataCatalog {
        movie_detail_calls: Arc<Mutex<Vec<u32>>>,
        tv_detail_calls: Arc<Mutex<Vec<u32>>>,
        movie_search_calls: Arc<Mutex<Vec<(String, String)>>>,
    }

    impl MetadataCatalog for FakeMetadataCatalog {
        async fn search_movie(&self, title: &str, year: &str) -> AppResult<Vec<SearchMovieResult>> {
            self.movie_search_calls
                .lock()
                .unwrap()
                .push((title.to_string(), year.to_string()));
            Ok(vec![SearchMovieResult {
                id: 7,
                title: title.to_string(),
                original_title: title.to_string(),
            }])
        }

        async fn get_movie_detail(&self, id: u32) -> AppResult<Option<MovieDetail>> {
            self.movie_detail_calls.lock().unwrap().push(id);
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
}
