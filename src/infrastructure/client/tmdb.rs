use serde::{Deserialize, de::DeserializeOwned};

use super::{RequestError, RequestResult};

const TMDB_HOST: &str = "https://api.themoviedb.org/3";

#[derive(Debug, Default, Clone, Deserialize)]
#[serde(default)]
pub struct Genre {
    pub id: u32,
    pub name: String,
}

#[derive(Debug, Default, Clone, Deserialize)]
#[serde(default)]
pub struct MovieDetail {
    pub id: u32,
    pub title: String,
    pub adult: bool,
    pub genres: Vec<Genre>,
    pub original_language: String,
    pub original_title: String,
    pub origin_country: Vec<String>,
    pub release_date: String,
}
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct SearchMovieResult {
    pub id: u32,
    pub title: String,
    pub original_title: String,
    pub release_date: String,
    pub poster_path: Option<String>,
    pub overview: String,
}

#[derive(Debug, Default, Deserialize)]
struct SearchMovieResponse {
    pub results: Vec<SearchMovieResult>,
}

#[derive(Debug, Default, Clone, Deserialize)]
#[serde(default)]
pub struct Season {
    pub id: u32,
    pub name: String,
    pub episode_count: u32,
    pub season_number: u32,
}

#[derive(Debug, Default, Clone, Deserialize)]
#[serde(default)]
pub struct TvDetail {
    pub id: u32,
    pub name: String,
    pub first_air_date: String,
    pub number_of_episodes: u32,
    pub number_of_seasons: u32,
    pub origin_country: Vec<String>,
    pub original_language: String,
    pub original_name: String,
    pub genres: Vec<Genre>,
    pub seasons: Vec<Season>,
}

#[derive(Debug, Deserialize)]
struct SearchTvResponse {
    pub results: Vec<SearchTvResult>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct SearchTvResult {
    pub id: u32,
    pub name: String,
    pub original_name: String,
    pub first_air_date: String,
    pub poster_path: Option<String>,
    pub overview: String,
}

#[derive(Debug, Default, Clone)]
pub struct Client {
    api_key: String,
    host: String,
}

impl Client {
    pub fn new(api_key: &str) -> Self {
        Client {
            api_key: api_key.to_owned(),
            host: TMDB_HOST.to_owned(),
        }
    }

    #[cfg(test)]
    fn with_host(api_key: &str, host: &str) -> Self {
        Client {
            api_key: api_key.to_owned(),
            host: host.to_owned(),
        }
    }

    async fn get<T: DeserializeOwned>(
        &self,
        url: &str,
        query: Option<Vec<(&str, &str)>>,
    ) -> RequestResult<T> {
        let mut request_query = vec![
            ("language", "zh-CN"),
            ("include_adult", "true"),
            ("api_key", &self.api_key),
        ];
        if let Some(q) = query {
            request_query.extend(q);
        }

        super::http::get(format!("{}{}", self.host, url), Some(request_query), None).await
    }

    pub async fn search_movie(
        &self,
        query: &str,
        year: &str,
    ) -> RequestResult<Vec<SearchMovieResult>> {
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
            Err(RequestError::NotFound(_)) => Ok(None),
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
            Err(RequestError::NotFound(_)) => Ok(None),
            Err(e) => Err(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{method, path, query_param},
    };

    fn client(server: &MockServer) -> Client {
        Client::with_host("test-api-key", server.uri().as_str())
    }

    #[tokio::test]
    async fn search_movie_sends_expected_query_and_returns_results() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/search/movie"))
            .and(query_param("query", "Inception"))
            .and(query_param("primary_release_year", "2010"))
            .and(query_param("language", "zh-CN"))
            .and(query_param("include_adult", "true"))
            .and(query_param("api_key", "test-api-key"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "results": [{
                    "id": 27205,
                    "title": "Inception",
                    "original_title": "Inception",
                    "release_date": "2010-07-16",
                    "poster_path": "/inception.jpg",
                    "overview": "A thief who steals corporate secrets."
                }]
            })))
            .mount(&server)
            .await;

        let result = client(&server)
            .search_movie("Inception", "2010")
            .await
            .unwrap();

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].id, 27205);
        assert_eq!(result[0].title, "Inception");
        assert_eq!(result[0].release_date, "2010-07-16");
        assert_eq!(result[0].poster_path.as_deref(), Some("/inception.jpg"));
        assert_eq!(result[0].overview, "A thief who steals corporate secrets.");
    }

    #[tokio::test]
    async fn get_movie_detail_returns_none_on_not_found() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/movie/999"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&server)
            .await;

        let result = client(&server).get_movie_detail(999).await.unwrap();

        assert!(result.is_none());
    }

    #[tokio::test]
    async fn get_movie_detail_returns_detail_on_success() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/movie/27205"))
            .and(query_param("language", "zh-CN"))
            .and(query_param("include_adult", "true"))
            .and(query_param("api_key", "test-api-key"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "id": 27205,
                "title": "Inception",
                "adult": false,
                "genres": [{"id": 1, "name": "Sci-Fi"}],
                "original_language": "en",
                "original_title": "Inception",
                "origin_country": ["US"],
                "release_date": "2010-07-16"
            })))
            .mount(&server)
            .await;

        let result = client(&server)
            .get_movie_detail(27205)
            .await
            .unwrap()
            .unwrap();

        assert_eq!(result.id, 27205);
        assert_eq!(result.title, "Inception");
        assert_eq!(result.genres.len(), 1);
    }

    #[tokio::test]
    async fn search_tv_sends_expected_query_and_returns_results() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/search/tv"))
            .and(query_param("query", "Breaking Bad"))
            .and(query_param("first_air_date_year", "2008"))
            .and(query_param("language", "zh-CN"))
            .and(query_param("include_adult", "true"))
            .and(query_param("api_key", "test-api-key"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "results": [{
                    "id": 1396,
                    "name": "Breaking Bad",
                    "original_name": "Breaking Bad",
                    "first_air_date": "2008-01-20",
                    "poster_path": "/breaking-bad.jpg",
                    "overview": "A chemistry teacher turned meth maker."
                }]
            })))
            .mount(&server)
            .await;

        let result = client(&server)
            .search_tv("Breaking Bad", "2008")
            .await
            .unwrap();

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].id, 1396);
        assert_eq!(result[0].name, "Breaking Bad");
        assert_eq!(result[0].first_air_date, "2008-01-20");
        assert_eq!(result[0].poster_path.as_deref(), Some("/breaking-bad.jpg"));
        assert_eq!(result[0].overview, "A chemistry teacher turned meth maker.");
    }

    #[tokio::test]
    async fn get_tv_detail_returns_none_on_not_found() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/tv/999"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&server)
            .await;

        let result = client(&server).get_tv_detail(999).await.unwrap();

        assert!(result.is_none());
    }

    #[tokio::test]
    async fn get_tv_detail_propagates_decode_error() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/tv/1396"))
            .respond_with(ResponseTemplate::new(200).set_body_string("not json"))
            .mount(&server)
            .await;

        let error = client(&server).get_tv_detail(1396).await.unwrap_err();

        match error {
            RequestError::Other(message) => {
                assert!(message.contains("decode payload failed"));
            }
            other => panic!("expected decode error, got {other:?}"),
        }
    }
}
