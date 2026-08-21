use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use chrono::Utc;
use sea_orm::Database;
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{method, path, query_param},
};

use super::*;
use crate::{
    application::ports::MetadataCatalog,
    infrastructure::{cache::Cache, client::tmdb, entity::cache as cache_entity},
    migration::{Migrator, MigratorTrait},
};

async fn setup_db() -> sea_orm::DatabaseConnection {
    let db = Database::connect("sqlite::memory:").await.unwrap();
    Migrator::up(&db, None).await.unwrap();
    db
}

async fn cached_gateway(server: &MockServer) -> (TmdbMetadataGateway, sea_orm::DatabaseConnection) {
    let db = setup_db().await;
    let gateway = TmdbMetadataGateway::new(tmdb::Client::with_host(
        "test-api-key",
        server.uri().as_str(),
    ))
    .with_cache(Cache::new(db.clone()));
    (gateway, db)
}

fn assert_ttl_close(expired_at: chrono::DateTime<Utc>, expected: Duration) {
    let remaining = (expired_at - Utc::now()).num_seconds();
    let expected_secs = expected.as_secs() as i64;
    assert!(
        (expected_secs - 5..=expected_secs).contains(&remaining),
        "ttl remaining {remaining}s, expected ~{expected_secs}s"
    );
}

async fn expired_at_for(db: &sea_orm::DatabaseConnection, key: &str) -> chrono::DateTime<Utc> {
    cache_entity::get_by_key(db, key)
        .await
        .unwrap()
        .unwrap()
        .expired_at
        .unwrap()
}

#[test]
fn search_cache_keys_are_hashed_and_distinct() {
    let movie_2010 = tmdb_search_cache_key("movie", "Inception", "2010");
    let movie_empty = tmdb_search_cache_key("movie", "Inception", "");
    let movie_colon = tmdb_search_cache_key("movie", "foo:bar", "");
    let movie_split = tmdb_search_cache_key("movie", "foo", "bar");
    let tv_empty = tmdb_search_cache_key("tv", "绝命毒师", "");

    assert_eq!(movie_2010.len(), "tmdb:movie:search:".len() + 64);
    assert_eq!(movie_empty.len(), "tmdb:movie:search:".len() + 64);
    assert_eq!(tv_empty.len(), "tmdb:tv:search:".len() + 64);
    assert!(movie_2010.starts_with("tmdb:movie:search:"));
    assert!(tv_empty.starts_with("tmdb:tv:search:"));
    assert_ne!(movie_2010, movie_empty);
    assert_ne!(movie_colon, movie_split);
    assert_ne!(movie_empty, tv_empty);
}

#[tokio::test]
async fn search_movie_uses_cache_on_second_call() {
    let server = MockServer::start().await;
    let (gateway, _) = cached_gateway(&server).await;

    Mock::given(method("GET"))
        .and(path("/search/movie"))
        .and(query_param("query", "Inception"))
        .and(query_param("primary_release_year", "2010"))
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
        .expect(1)
        .mount(&server)
        .await;

    let first = gateway.search_movie("Inception", "2010").await.unwrap();
    let second = gateway.search_movie("Inception", "2010").await.unwrap();

    assert_eq!(first.len(), 1);
    assert_eq!(first[0].id, 27205);
    assert_eq!(second[0].title, "Inception");
}

#[tokio::test]
async fn search_tv_uses_cache_on_second_call() {
    let server = MockServer::start().await;
    let (gateway, _) = cached_gateway(&server).await;

    Mock::given(method("GET"))
        .and(path("/search/tv"))
        .and(query_param("query", "Breaking Bad"))
        .and(query_param("first_air_date_year", "2008"))
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
        .expect(1)
        .mount(&server)
        .await;

    let first = gateway.search_tv("Breaking Bad", "2008").await.unwrap();
    let second = gateway.search_tv("Breaking Bad", "2008").await.unwrap();

    assert_eq!(first[0].id, 1396);
    assert_eq!(second[0].name, "Breaking Bad");
}

#[tokio::test]
async fn movie_detail_uses_cache_on_second_call() {
    let server = MockServer::start().await;
    let (gateway, _) = cached_gateway(&server).await;

    Mock::given(method("GET"))
        .and(path("/movie/27205"))
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
        .expect(1)
        .mount(&server)
        .await;

    let first = gateway.get_movie_detail(27205).await.unwrap().unwrap();
    let second = gateway.get_movie_detail(27205).await.unwrap().unwrap();

    assert_eq!(first.id, 27205);
    assert_eq!(second.title, "Inception");
}

#[tokio::test]
async fn tv_detail_uses_cache_on_second_call() {
    let server = MockServer::start().await;
    let (gateway, _) = cached_gateway(&server).await;

    Mock::given(method("GET"))
        .and(path("/tv/1396"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": 1396,
            "name": "Breaking Bad",
            "first_air_date": "2008-01-20",
            "number_of_episodes": 62,
            "number_of_seasons": 5,
            "origin_country": ["US"],
            "original_language": "en",
            "original_name": "Breaking Bad",
            "genres": [],
            "seasons": []
        })))
        .expect(1)
        .mount(&server)
        .await;

    let first = gateway.get_tv_detail(1396).await.unwrap().unwrap();
    let second = gateway.get_tv_detail(1396).await.unwrap().unwrap();

    assert_eq!(first.id, 1396);
    assert_eq!(second.number_of_seasons, 5);
}

#[tokio::test]
async fn empty_search_and_missing_detail_are_cached() {
    let server = MockServer::start().await;
    let (gateway, _) = cached_gateway(&server).await;

    Mock::given(method("GET"))
        .and(path("/search/movie"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "results": []
        })))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/movie/999"))
        .respond_with(ResponseTemplate::new(404))
        .expect(1)
        .mount(&server)
        .await;

    assert!(
        gateway
            .search_movie("Nope", "1999")
            .await
            .unwrap()
            .is_empty()
    );
    assert!(
        gateway
            .search_movie("Nope", "1999")
            .await
            .unwrap()
            .is_empty()
    );
    assert!(gateway.get_movie_detail(999).await.unwrap().is_none());
    assert!(gateway.get_movie_detail(999).await.unwrap().is_none());
}

#[tokio::test]
async fn cache_ttls_depend_on_payload() {
    let server = MockServer::start().await;
    let (gateway, db) = cached_gateway(&server).await;

    Mock::given(method("GET"))
        .and(path("/search/movie"))
        .and(query_param("query", "Inception"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "results": [{
                "id": 27205,
                "title": "Inception",
                "original_title": "Inception",
                "release_date": "2010-07-16",
                "poster_path": "/inception.jpg",
                "overview": "overview"
            }]
        })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/search/movie"))
        .and(query_param("query", "Missing"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "results": []
        })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/movie/27205"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": 27205,
            "title": "Inception",
            "adult": false,
            "genres": [],
            "original_language": "en",
            "original_title": "Inception",
            "origin_country": ["US"],
            "release_date": "2010-07-16"
        })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/tv/1396"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": 1396,
            "name": "Breaking Bad",
            "first_air_date": "2008-01-20",
            "number_of_episodes": 62,
            "number_of_seasons": 5,
            "origin_country": ["US"],
            "original_language": "en",
            "original_name": "Breaking Bad",
            "genres": [],
            "seasons": []
        })))
        .mount(&server)
        .await;

    gateway.search_movie("Inception", "2010").await.unwrap();
    gateway.search_movie("Missing", "").await.unwrap();
    gateway.get_movie_detail(27205).await.unwrap();
    gateway.get_tv_detail(1396).await.unwrap();

    assert_ttl_close(
        expired_at_for(&db, &tmdb_search_cache_key("movie", "Inception", "2010")).await,
        TMDB_SEARCH_TTL,
    );
    assert_ttl_close(
        expired_at_for(&db, &tmdb_search_cache_key("movie", "Missing", "")).await,
        TMDB_EMPTY_TTL,
    );
    assert_ttl_close(
        expired_at_for(&db, &tmdb_movie_detail_cache_key(27205)).await,
        TMDB_MOVIE_DETAIL_TTL,
    );
    assert_ttl_close(
        expired_at_for(&db, &tmdb_tv_detail_cache_key(1396)).await,
        TMDB_TV_DETAIL_TTL,
    );
}

#[tokio::test]
async fn server_error_is_not_cached() {
    let server = MockServer::start().await;
    let (gateway, _) = cached_gateway(&server).await;
    let calls = AtomicUsize::new(0);

    Mock::given(method("GET"))
        .and(path("/search/movie"))
        .respond_with(move |_: &wiremock::Request| {
            let index = calls.fetch_add(1, Ordering::SeqCst);
            // HTTP client retries transient 5xx (1 attempt + 3 retries).
            if index < 4 {
                ResponseTemplate::new(500)
            } else {
                ResponseTemplate::new(200).set_body_json(serde_json::json!({
                    "results": [{
                        "id": 1,
                        "title": "Later",
                        "original_title": "Later",
                        "release_date": "2020-01-01",
                        "poster_path": null,
                        "overview": ""
                    }]
                }))
            }
        })
        .expect(5)
        .mount(&server)
        .await;

    assert!(gateway.search_movie("Later", "2020").await.is_err());
    let results = gateway.search_movie("Later", "2020").await.unwrap();
    assert_eq!(results[0].id, 1);
}

#[tokio::test]
async fn expired_cache_entry_refetches() {
    let server = MockServer::start().await;
    let (gateway, db) = cached_gateway(&server).await;
    let key = tmdb_search_cache_key("movie", "Inception", "2010");
    Cache::new(db)
        .set(
            &key,
            &vec![tmdb::SearchMovieResult {
                id: 1,
                title: "stale".into(),
                original_title: "stale".into(),
                release_date: "2000-01-01".into(),
                poster_path: None,
                overview: String::new(),
            }],
            Some(Duration::from_millis(1)),
        )
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(20)).await;

    Mock::given(method("GET"))
        .and(path("/search/movie"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "results": [{
                "id": 27205,
                "title": "Inception",
                "original_title": "Inception",
                "release_date": "2010-07-16",
                "poster_path": "/inception.jpg",
                "overview": "overview"
            }]
        })))
        .expect(1)
        .mount(&server)
        .await;

    let results = gateway.search_movie("Inception", "2010").await.unwrap();
    assert_eq!(results[0].id, 27205);
}

#[tokio::test]
async fn invalid_cache_json_is_deleted_and_refetched() {
    let server = MockServer::start().await;
    let (gateway, db) = cached_gateway(&server).await;
    let key = tmdb_movie_detail_cache_key(27205);
    cache_entity::set_record(
        &db,
        &key,
        "not-json",
        Some(Utc::now() + chrono::Duration::hours(1)),
    )
    .await
    .unwrap();

    Mock::given(method("GET"))
        .and(path("/movie/27205"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": 27205,
            "title": "Inception",
            "adult": false,
            "genres": [],
            "original_language": "en",
            "original_title": "Inception",
            "origin_country": ["US"],
            "release_date": "2010-07-16"
        })))
        .expect(1)
        .mount(&server)
        .await;

    let detail = gateway.get_movie_detail(27205).await.unwrap().unwrap();
    assert_eq!(detail.title, "Inception");
}

#[tokio::test]
async fn gateway_without_cache_still_fetches() {
    let server = MockServer::start().await;
    let gateway = TmdbMetadataGateway::new(tmdb::Client::with_host(
        "test-api-key",
        server.uri().as_str(),
    ));

    Mock::given(method("GET"))
        .and(path("/search/tv"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "results": [{
                "id": 1396,
                "name": "Breaking Bad",
                "original_name": "Breaking Bad",
                "first_air_date": "2008-01-20",
                "poster_path": "/breaking-bad.jpg",
                "overview": "overview"
            }]
        })))
        .expect(2)
        .mount(&server)
        .await;

    assert_eq!(
        gateway.search_tv("Breaking Bad", "").await.unwrap()[0].id,
        1396
    );
    assert_eq!(
        gateway.search_tv("Breaking Bad", "").await.unwrap()[0].id,
        1396
    );
}
