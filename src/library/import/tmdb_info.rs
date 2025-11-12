use tracing::info;

use crate::{
    client::tmdb::{MovieDetail, TvDetail},
    error::AppResult,
    media::Metadata,
};

use super::Importer;

impl Importer {
    pub(super) async fn get_movie_info_from_tmdb(&mut self, meta: &Metadata) -> AppResult<Option<MovieDetail>> {
        if !meta.tmdb_id.is_empty() {
            let cache_key = format!("movie:{}", meta.tmdb_id);
            if let Some(movie) = self.movie_info_cache.get(&cache_key) {
                return Ok(movie.clone());
            }

            if let Some(movie) = self.state.tmdb.get_movie_detail(meta.tmdb_id.parse().unwrap()).await? {
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
            let movies = self.state.tmdb.search_movie(&title.title, &meta.year).await?;
            match movies.len() {
                0 => {
                    self.movie_info_cache.insert(cache_key, None);
                    continue;
                }
                1 => {
                    info!(
                        "Movie found for title: {}, year: {}, id: {}",
                        title.title, meta.year, movies[0].id
                    );
                    let movie = self.state.tmdb.get_movie_detail(movies[0].id).await?;
                    self.movie_info_cache.insert(cache_key, movie.clone());
                    return Ok(movie);
                }
                _ => {
                    for movie in &movies {
                        if movie.original_title == title.title || movie.title == title.title {
                            info!(
                                "Movie found for title: {}, year: {}, id: {}",
                                title.title, meta.year, movie.id
                            );
                            let movie = self.state.tmdb.get_movie_detail(movie.id).await?;
                            self.movie_info_cache.insert(cache_key, movie.clone());
                            return Ok(movie);
                        }
                    }

                    self.movie_info_cache.insert(cache_key, None);
                    continue;
                }
            }
        }
        Ok(None)
    }

    pub(super) async fn get_tv_info_from_tmdb(&mut self, meta: &Metadata) -> AppResult<Option<TvDetail>> {
        if !meta.tmdb_id.is_empty() {
            let cache_key = format!("tv:{}", meta.tmdb_id);
            if let Some(tv) = self.tv_info_cache.get(&cache_key) {
                return Ok(tv.clone());
            }

            if let Some(tv) = self.state.tmdb.get_tv_detail(meta.tmdb_id.parse().unwrap()).await? {
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
            let tvs = self.state.tmdb.search_tv(&title.title, &meta.year).await?;
            match tvs.len() {
                0 => {
                    self.tv_info_cache.insert(cache_key, None);
                    continue;
                }
                1 => {
                    info!(
                        "Tv found for title: {}, year: {}, id: {}",
                        title.title, meta.year, tvs[0].id
                    );
                    let tv = self.state.tmdb.get_tv_detail(tvs[0].id).await?;
                    self.tv_info_cache.insert(cache_key, tv.clone());
                    return Ok(tv);
                }
                _ => {
                    for tv in &tvs {
                        if tv.original_name == title.title || tv.name == title.title {
                            info!(
                                "Tv found for title: {}, year: {}, id: {}",
                                title.title, meta.year, tv.id
                            );
                            let tv = self.state.tmdb.get_tv_detail(tv.id).await?;
                            self.tv_info_cache.insert(cache_key, tv.clone());
                            return Ok(tv);
                        }
                    }

                    self.tv_info_cache.insert(cache_key, None);
                    continue;
                }
            }
        }
        Ok(None)
    }
}
