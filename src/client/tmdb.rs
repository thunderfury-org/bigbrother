use serde::{Deserialize, de::DeserializeOwned};

use super::{RequestError, RequestResult};

const TMDB_HOST: &str = "https://api.themoviedb.org/3";

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct Genre {
    pub id: u32,
    pub name: String,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct MovieDetail {
    pub id: u32,
    pub title: String,
    pub adult: bool,
    pub genres: Vec<Genre>,
    pub original_language: String,
    pub original_title: String,
    pub overview: String,
    pub origin_country: Vec<String>,
    pub release_date: String,
}
#[derive(Debug, Default, Deserialize)]
pub struct SearchMovieResult {
    pub id: u32,
    pub title: String,
    pub original_title: String,
    pub original_language: String,
    pub release_date: String,
}

#[derive(Debug, Default, Deserialize)]
struct SearchMovieResponse {
    pub results: Vec<SearchMovieResult>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct Season {
    pub id: u32,
    pub name: String,
    pub episode_count: u32,
    pub air_date: String,
    pub season_number: u32,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct TvDetail {
    pub id: u32,
    pub name: String,
    pub first_air_date: String,
    pub number_of_episodes: u32,
    pub number_of_seasons: u32,
    pub original_country: Vec<String>,
    pub original_language: String,
    pub original_name: String,
    pub genres: Vec<Genre>,
    pub seasons: Vec<Season>,
}

#[derive(Debug, Deserialize)]
struct SearchTvResponse {
    pub results: Vec<SearchTvResult>,
}

#[derive(Debug, Deserialize)]
pub struct SearchTvResult {
    pub id: u32,
    pub name: String,
    pub first_air_date: String,
}

pub struct Client {
    api_key: String,
}

impl Client {
    async fn get<T: DeserializeOwned>(&self, url: &str, query: Option<Vec<(&str, &str)>>) -> RequestResult<T> {
        let mut request_query = vec![
            ("language", "zh-CN"),
            ("include_adult", "true"),
            ("api_key", &self.api_key),
        ];
        if let Some(q) = query {
            request_query.extend(q);
        }

        super::http::get(format!("{TMDB_HOST}{url}"), Some(request_query), None).await
    }

    pub async fn search_movie(&self, query: &str, year: &str) -> RequestResult<Vec<SearchMovieResult>> {
        self.get::<SearchMovieResponse>(
            "/search/movie",
            Some(vec![("query", query), ("primary_release_year", year)]),
        )
        .await
        .map(|resp| resp.results)
    }

    pub async fn get_movie_detail(&self, id: u32) -> RequestResult<Option<MovieDetail>> {
        match self.get(&format!("/movie/{}", id), None).await {
            Ok(detail) => Ok(Some(detail)),
            Err(RequestError::NotFound) => Ok(None),
            Err(e) => Err(e),
        }
    }

    pub async fn search_tv(&self, query: &str, year: &str) -> RequestResult<Vec<SearchTvResult>> {
        self.get::<SearchTvResponse>(
            "/search/tv",
            Some(vec![("query", query), ("first_air_date_year", year)]),
        )
        .await
        .map(|resp| resp.results)
    }

    pub async fn get_tv_detail(&self, id: u32) -> RequestResult<Option<TvDetail>> {
        match self.get(&format!("/tv/{}", id), None).await {
            Ok(detail) => Ok(Some(detail)),
            Err(RequestError::NotFound) => Ok(None),
            Err(e) => Err(e),
        }
    }
}
