mod id;
mod resolve;

use std::collections::HashMap;

use tracing::info;

use crate::{
    application::import_ports::MetadataCatalog,
    domain::{
        import::{MovieDetail, TvDetail},
        media::Metadata,
    },
    error::AppResult,
};
use id::parsed_tmdb_id;
use resolve::{resolve_movie_candidate, resolve_tv_candidate};

#[derive(Clone)]
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
            match resolve_movie_candidate(&title.title, &meta.year, movies, &self.metadata_catalog)
                .await?
            {
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
            match resolve_tv_candidate(&title.title, &meta.year, tvs, &self.metadata_catalog)
                .await?
            {
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

#[cfg(test)]
#[path = "tmdb_info/tests.rs"]
mod tests;
