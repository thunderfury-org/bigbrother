use tracing::info;

use crate::{
    client::tmdb::{MovieDetail, TvDetail},
    error::AppResult,
    media::Title,
};

use super::Importer;

impl Importer {
    pub(super) async fn get_movie_info_from_tmdb(
        &mut self,
        titles: &Vec<Title>,
        year: &str,
    ) -> AppResult<Option<MovieDetail>> {
        for title in titles {
            let cache_key = format!("movie:{}:{}", title.title, year);
            if let Some(movie) = self.movie_info_cache.get(&cache_key) {
                return Ok(movie.clone());
            }
            let movies = self.state.tmdb.search_movie(&title.title, year).await?;
            match movies.len() {
                0 => {
                    info!("No movie found for title: {}, year: {}", title.title, year);
                    self.movie_info_cache.insert(cache_key, None);
                    continue;
                }
                1 => {
                    info!(
                        "Movie found for title: {}, year: {}, id: {}",
                        title.title, year, movies[0].id
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
                                title.title, year, movie.id
                            );
                            let movie = self.state.tmdb.get_movie_detail(movie.id).await?;
                            self.movie_info_cache.insert(cache_key, movie.clone());
                            return Ok(movie);
                        }
                    }

                    info!("No movie found for title: {}, year: {}", title.title, year);
                    self.movie_info_cache.insert(cache_key, None);
                    continue;
                }
            }
        }
        Ok(None)
    }

    pub(super) async fn get_tv_info_from_tmdb(
        &mut self,
        titles: &Vec<Title>,
        year: &str,
    ) -> AppResult<Option<TvDetail>> {
        for title in titles {
            let cache_key = format!("tv:{}:{}", title.title, year);
            if let Some(tv) = self.tv_info_cache.get(&cache_key) {
                return Ok(tv.clone());
            }
            let tvs = self.state.tmdb.search_tv(&title.title, year).await?;
            match tvs.len() {
                0 => {
                    info!("No tv found for title: {}, year: {}", title.title, year);
                    self.tv_info_cache.insert(cache_key, None);
                    continue;
                }
                1 => {
                    info!("Tv found for title: {}, year: {}, id: {}", title.title, year, tvs[0].id);
                    let tv = self.state.tmdb.get_tv_detail(tvs[0].id).await?;
                    self.tv_info_cache.insert(cache_key, tv.clone());
                    return Ok(tv);
                }
                _ => {
                    for tv in &tvs {
                        if tv.original_name == title.title || tv.name == title.title {
                            info!("Tv found for title: {}, year: {}, id: {}", title.title, year, tv.id);
                            let tv = self.state.tmdb.get_tv_detail(tv.id).await?;
                            self.tv_info_cache.insert(cache_key, tv.clone());
                            return Ok(tv);
                        }
                    }

                    info!("No tv found for title: {}, year: {}", title.title, year);
                    self.tv_info_cache.insert(cache_key, None);
                    continue;
                }
            }
        }
        Ok(None)
    }
}
