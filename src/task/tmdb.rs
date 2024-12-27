use std::fmt::Display;

use reqwest::{IntoUrl, StatusCode};
use serde::{de::DeserializeOwned, Deserialize};

use crate::common::{
    error::{Error, Result},
    state::AppState,
};

const TMDB_HOST: &str = "https://api.themoviedb.org/3";

pub struct Client<'a> {
    client: reqwest::Client,
    api_key: &'a str,
}

impl<'a> Client<'a> {
    async fn get<U: IntoUrl + Display, T: DeserializeOwned>(
        &self,
        url: U,
        query: Option<Vec<(&str, &str)>>,
    ) -> Result<T> {
        let mut request_query = vec![("language", "zh-CN"), ("api_key", self.api_key)];
        if let Some(q) = query {
            request_query.extend(q);
        }

        let result = self
            .client
            .get(format!("{}{}", TMDB_HOST, url))
            .query(&request_query)
            .send()
            .await;
        if result.is_err() {
            return Err(Error::Internal(format!(
                "http get {} failed, {}",
                url,
                result.err().unwrap()
            )));
        }

        let response = result.unwrap();
        let status = response.status();
        let body = response.text().await?;

        if status.is_success() {
            return serde_json::from_str(&body)
                .map_err(|e| Error::Internal(format!("http get {url} failed, decode body failed, {e}, body: {body}")));
        }

        match status {
            StatusCode::NOT_FOUND => Err(Error::NotFound("tmdb not found".to_string())),
            _ => Err(Error::Internal(format!(
                "http get {url} failed, status: {status}, body: {body}"
            ))),
        }
    }

    pub async fn get_tv_detail(&self, tv_id: i32) -> Result<TvDetail> {
        match self.get(format!("/tv/{}", tv_id), None).await {
            Ok(detail) => Ok(detail),
            Err(Error::NotFound(_)) => Err(Error::NotFound(format!("can not find tv {} in tmdb", tv_id))),
            Err(e) => Err(e),
        }
    }

    pub async fn search_tv(&self, query: &str) -> Result<Vec<SearchTvResult>> {
        let response: SearchTvResponse = self
            .get(
                "/search/tv",
                Some(vec![("query", query), ("include_adult", "true"), ("page", "1")]),
            )
            .await?;
        Ok(response.results)
    }
}

impl<'a> TryFrom<&'a AppState> for Client<'a> {
    type Error = Error;

    fn try_from(state: &'a AppState) -> Result<Self> {
        let api_key = state.config.get_app_config().tmdb_api_key.as_str();
        if api_key.is_empty() {
            return Err(Error::Internal("tmdb api key is empty".to_string()));
        }

        Ok(Self {
            client: state.http_client.clone(),
            api_key: state.config.get_app_config().tmdb_api_key.as_str(),
        })
    }
}

#[derive(Debug, Deserialize)]
pub struct TvDetail {
    // pub id: i32,
    pub name: String,
    // pub status: String,
    // pub adult: bool,
    pub first_air_date: String,
    // pub in_production: bool,
    // pub last_air_date: String,
    // pub number_of_episodes: i32,
    pub number_of_seasons: i32,
    // pub original_language: String,
    // pub original_name: String,
}

#[derive(Debug, Deserialize)]
pub struct SearchTvResponse {
    pub results: Vec<SearchTvResult>,
}

#[derive(Debug, Deserialize)]
pub struct SearchTvResult {
    pub id: i32,
    // pub name: String,
    // pub first_air_date: String,
}
