use tracing::info;

use crate::{
    application::import_ports::MetadataCatalog,
    domain::import::{MovieDetail, SearchMovieResult, SearchTvResult, TvDetail},
    error::AppResult,
};

pub(super) async fn resolve_movie_candidate<M>(
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

pub(super) async fn resolve_tv_candidate<M>(
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
