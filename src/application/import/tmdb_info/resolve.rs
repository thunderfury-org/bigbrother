use tracing::info;

use crate::{
    application::ports::MetadataCatalog,
    domain::import::{MovieDetail, SearchMovieResult, SearchTvResult, TvDetail},
    error::AppResult,
};

pub(super) async fn resolve_movie_candidate<M>(
    title: &str,
    year: &str,
    movies: Vec<SearchMovieResult>,
    metadata_catalog: &M,
) -> AppResult<Option<MovieDetail>>
where
    M: MetadataCatalog,
{
    let normalized_title = normalize_title(title);

    match movies.len() {
        0 => Ok(None),
        1 => {
            info!("Movie found for title: {}, id: {}", title, movies[0].id);
            metadata_catalog.get_movie_detail(movies[0].id).await
        }
        _ => {
            let matched = movies
                .iter()
                .filter(|movie| {
                    normalize_title(movie.original_title.as_str()) == normalized_title
                        || normalize_title(movie.title.as_str()) == normalized_title
                })
                .collect::<Vec<_>>();

            if matched.is_empty() {
                return Ok(None);
            }

            if !year.is_empty() {
                for movie in &matched {
                    if let Some(detail) = metadata_catalog.get_movie_detail(movie.id).await?
                        && detail
                            .release_date
                            .get(..4)
                            .is_some_and(|release_year| release_year == year)
                    {
                        info!("Movie found for title: {}, id: {}", title, movie.id);
                        return Ok(Some(detail));
                    }
                }
            }

            for movie in matched {
                if let Some(detail) = metadata_catalog.get_movie_detail(movie.id).await? {
                    info!("Movie found for title: {}, id: {}", title, movie.id);
                    return Ok(Some(detail));
                }
            }

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
    year: &str,
    tvs: Vec<SearchTvResult>,
    metadata_catalog: &M,
) -> AppResult<Option<TvDetail>>
where
    M: MetadataCatalog,
{
    let normalized_title = normalize_title(title);

    match tvs.len() {
        0 => Ok(None),
        1 => {
            info!("Tv found for title: {}, id: {}", title, tvs[0].id);
            metadata_catalog.get_tv_detail(tvs[0].id).await
        }
        _ => {
            let matched = tvs
                .iter()
                .filter(|tv| {
                    normalize_title(tv.original_name.as_str()) == normalized_title
                        || normalize_title(tv.name.as_str()) == normalized_title
                })
                .collect::<Vec<_>>();

            if matched.is_empty() {
                return Ok(None);
            }

            if !year.is_empty() {
                for tv in &matched {
                    if let Some(detail) = metadata_catalog.get_tv_detail(tv.id).await?
                        && detail
                            .first_air_date
                            .get(..4)
                            .is_some_and(|release_year| release_year == year)
                    {
                        info!("Tv found for title: {}, id: {}", title, tv.id);
                        return Ok(Some(detail));
                    }
                }
            }

            for tv in matched {
                if let Some(detail) = metadata_catalog.get_tv_detail(tv.id).await? {
                    info!("Tv found for title: {}, id: {}", title, tv.id);
                    return Ok(Some(detail));
                }
            }

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

fn normalize_title(title: &str) -> String {
    let mut normalized = String::with_capacity(title.len());
    for ch in title.chars() {
        if ch.is_alphanumeric() || !ch.is_ascii() && ch.is_alphabetic() {
            normalized.extend(ch.to_lowercase());
        } else {
            normalized.push(' ');
        }
    }

    normalized.split_whitespace().collect::<Vec<_>>().join(" ")
}
