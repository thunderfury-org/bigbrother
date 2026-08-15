mod id;
mod resolve;

use std::collections::HashMap;

use tracing::info;

use std::sync::{Arc, Mutex};

use crate::{
    application::ports::{
        MetadataCatalog, MetadataCatalogHandle, TitleExtractor, TitleExtractorHandle,
    },
    domain::{
        import::{MovieDetail, TvDetail},
        media::Metadata,
    },
    error::AppResult,
};
use id::parsed_tmdb_id;
use resolve::{resolve_movie_candidate, resolve_tv_candidate};

#[derive(Clone)]
pub(super) struct TmdbLookup {
    metadata_catalog: MetadataCatalogHandle,
    title_extractor: TitleExtractorHandle,
    tv_info_cache: Arc<Mutex<HashMap<String, Option<TvDetail>>>>,
    movie_info_cache: Arc<Mutex<HashMap<String, Option<MovieDetail>>>>,
}

impl TmdbLookup {
    pub(super) fn new(
        metadata_catalog: impl MetadataCatalog + Send + Sync + 'static,
        title_extractor: impl TitleExtractor + Send + Sync + 'static,
    ) -> Self {
        Self {
            metadata_catalog: Arc::new(metadata_catalog),
            title_extractor: Arc::new(title_extractor),
            tv_info_cache: Arc::new(Mutex::new(HashMap::new())),
            movie_info_cache: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub(super) async fn get_movie_info(
        &self,
        meta: &Metadata,
        descriptions: &[String],
    ) -> AppResult<Option<MovieDetail>> {
        if let Some(tmdb_id) = parsed_tmdb_id(meta, "movie") {
            let cache_key = format!("movie:{}", meta.tmdb_id);
            if let Some(movie) = self
                .movie_info_cache
                .lock()
                .expect("movie cache")
                .get(&cache_key)
                .cloned()
            {
                return Ok(movie.clone());
            }

            if let Some(movie) = self.metadata_catalog.get_movie_detail(tmdb_id).await? {
                info!(
                    "Movie found for title: {}, year: {}, id: {}",
                    movie.title, movie.release_date, movie.id
                );
                self.movie_info_cache
                    .lock()
                    .expect("movie cache")
                    .insert(cache_key, Some(movie.clone()));
                return Ok(Some(movie));
            }
        }

        for title in &meta.titles {
            let cache_key = format!("movie:{}:{}", title.title, meta.year);
            if let Some(movie) = self
                .movie_info_cache
                .lock()
                .expect("movie cache")
                .get(&cache_key)
                .cloned()
            {
                if movie.is_some() {
                    return Ok(movie.clone());
                }
                continue;
            }
            let movies = self
                .metadata_catalog
                .search_movie(&title.title, &meta.year)
                .await?;
            match resolve_movie_candidate(
                &title.title,
                &meta.year,
                movies,
                self.metadata_catalog.as_ref(),
            )
            .await?
            {
                Some(movie) => {
                    self.movie_info_cache
                        .lock()
                        .expect("movie cache")
                        .insert(cache_key, Some(movie.clone()));
                    return Ok(Some(movie));
                }
                None => {
                    self.movie_info_cache
                        .lock()
                        .expect("movie cache")
                        .insert(cache_key, None);
                    continue;
                }
            }
        }

        // LLM fallback: try extracting title from descriptions
        for desc in descriptions.iter().filter(|d| !d.trim().is_empty()) {
            if let Some(extracted) = self.title_extractor.extract_title(desc).await? {
                let cache_key = format!("movie:{}:{}", extracted.title, meta.year);
                if let Some(cached) = self
                    .movie_info_cache
                    .lock()
                    .expect("movie cache")
                    .get(&cache_key)
                    .cloned()
                {
                    if cached.is_some() {
                        return Ok(cached);
                    }
                    continue;
                }
                let movies = self
                    .metadata_catalog
                    .search_movie(&extracted.title, &meta.year)
                    .await?;
                match resolve_movie_candidate(
                    &extracted.title,
                    &meta.year,
                    movies,
                    self.metadata_catalog.as_ref(),
                )
                .await?
                {
                    Some(movie) => {
                        self.movie_info_cache
                            .lock()
                            .expect("movie cache")
                            .insert(cache_key, Some(movie.clone()));
                        info!(
                            "Movie found via LLM title extraction: {}, id: {}",
                            movie.title, movie.id
                        );
                        return Ok(Some(movie));
                    }
                    None => {
                        self.movie_info_cache
                            .lock()
                            .expect("movie cache")
                            .insert(cache_key, None);
                        continue;
                    }
                }
            }
        }
        Ok(None)
    }

    pub(super) async fn get_tv_info(
        &self,
        meta: &Metadata,
        descriptions: &[String],
    ) -> AppResult<Option<TvDetail>> {
        if let Some(tmdb_id) = parsed_tmdb_id(meta, "tv") {
            let cache_key = format!("tv:{}", meta.tmdb_id);
            if let Some(tv) = self
                .tv_info_cache
                .lock()
                .expect("tv cache")
                .get(&cache_key)
                .cloned()
            {
                return Ok(tv.clone());
            }

            if let Some(tv) = self.metadata_catalog.get_tv_detail(tmdb_id).await? {
                info!(
                    "Tv found for title: {}, year: {}, id: {}",
                    tv.name, tv.first_air_date, tv.id
                );

                self.tv_info_cache
                    .lock()
                    .expect("tv cache")
                    .insert(cache_key, Some(tv.clone()));
                return Ok(Some(tv));
            }
        }

        for title in &meta.titles {
            let cache_key = format!("tv:{}:{}", title.title, meta.year);
            if let Some(tv) = self
                .tv_info_cache
                .lock()
                .expect("tv cache")
                .get(&cache_key)
                .cloned()
            {
                if tv.is_some() {
                    return Ok(tv.clone());
                }
                continue;
            }
            let tvs = self
                .metadata_catalog
                .search_tv(&title.title, &meta.year)
                .await?;
            match resolve_tv_candidate(
                &title.title,
                &meta.year,
                tvs,
                self.metadata_catalog.as_ref(),
            )
            .await?
            {
                Some(tv) => {
                    self.tv_info_cache
                        .lock()
                        .expect("tv cache")
                        .insert(cache_key, Some(tv.clone()));
                    return Ok(Some(tv));
                }
                None => {
                    self.tv_info_cache
                        .lock()
                        .expect("tv cache")
                        .insert(cache_key, None);
                    continue;
                }
            }
        }

        // LLM fallback: try extracting title from descriptions
        for desc in descriptions.iter().filter(|d| !d.trim().is_empty()) {
            if let Some(extracted) = self.title_extractor.extract_title(desc).await? {
                let cache_key = format!("tv:{}:{}", extracted.title, meta.year);
                if let Some(cached) = self
                    .tv_info_cache
                    .lock()
                    .expect("tv cache")
                    .get(&cache_key)
                    .cloned()
                {
                    if cached.is_some() {
                        return Ok(cached);
                    }
                    continue;
                }
                let tvs = self
                    .metadata_catalog
                    .search_tv(&extracted.title, &meta.year)
                    .await?;
                match resolve_tv_candidate(
                    &extracted.title,
                    &meta.year,
                    tvs,
                    self.metadata_catalog.as_ref(),
                )
                .await?
                {
                    Some(tv) => {
                        self.tv_info_cache
                            .lock()
                            .expect("tv cache")
                            .insert(cache_key, Some(tv.clone()));
                        info!(
                            "Tv found via LLM title extraction: {}, id: {}",
                            tv.name, tv.id
                        );
                        return Ok(Some(tv));
                    }
                    None => {
                        self.tv_info_cache
                            .lock()
                            .expect("tv cache")
                            .insert(cache_key, None);
                        continue;
                    }
                }
            }
        }
        Ok(None)
    }
}

#[cfg(test)]
#[path = "tmdb_info/tests.rs"]
mod tests;
