use std::fmt::Display;

use reqwest::{IntoUrl, StatusCode};
use serde::{Deserialize, de::DeserializeOwned};

const TMDB_HOST: &str = "https://api.themoviedb.org/3";

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct Genre {
    pub id: u32,
    pub name: String,
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
pub struct SearchTvResponse {
    pub results: Vec<SearchTvResult>,
}

#[derive(Debug, Deserialize)]
pub struct SearchTvResult {
    pub id: u32,
    pub name: String,
    pub first_air_date: String,
}

pub struct Error(String);

pub struct Client {
    client: reqwest::Client,
    api_key: String,
}

impl Client {
    async fn get<U: IntoUrl + Display, T: DeserializeOwned>(
        &self,
        url: U,
        query: Option<Vec<(&str, &str)>>,
    ) -> Result<Option<T>, Error> {
        let mut request_query = vec![
            ("language", "zh-CN"),
            ("include_adult", "true"),
            ("api_key", &self.api_key),
        ];
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
            return Err(Error(format!("http get {} failed, {}", url, result.err().unwrap())));
        }

        let response = result.unwrap();
        let status = response.status();
        let body = response.text().await?;

        if status.is_success() {
            return serde_json::from_str(&body)
                .map_err(|e| Error(format!("http get {url} failed, decode body failed, {e}, body: {body}")));
        }

        match status {
            StatusCode::NOT_FOUND => Ok(None),
            _ => Err(Error(format!("http get {url} failed, status: {status}, body: {body}"))),
        }
    }

    pub async fn get_tv_detail(&self, tv_id: u32) -> Result<TvDetail> {
        match self.get(format!("/tv/{}", tv_id), None).await {
            Ok(Some(detail)) => Ok(detail),
            Ok(None) => Err(Error(format!("can not find tv {} in tmdb", tv_id))),
            Err(e) => Err(e),
        }
    }

    pub async fn search_tv(&self, query: &str, year: Option<u32>) -> Result<Vec<SearchTvResult>> {
        let response: SearchTvResponse = self
            .get(
                "/search/tv",
                Some(vec![
                    ("query", query),
                    ("include_adult", "true"),
                    ("page", "1"),
                    ("first_air_date_year", &year.map(|y| y.to_string()).unwrap_or_default()),
                ]),
            )
            .await?;
        Ok(response.results)
    }
}
