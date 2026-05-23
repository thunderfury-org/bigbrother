use std::borrow::Cow;

use axum::{
    http::{StatusCode, header},
    response::{IntoResponse, Response},
};

pub(crate) struct AssetFile {
    pub bytes: Cow<'static, [u8]>,
    pub mime: Cow<'static, str>,
}

pub(crate) fn resolve_asset(path: &str, lookup: impl Fn(&str) -> Option<AssetFile>) -> Response {
    let trimmed = path.trim_start_matches('/');
    let lookup_key = if trimmed.is_empty() {
        "index.html"
    } else {
        trimmed
    };

    if let Some(asset) = lookup(lookup_key) {
        return ok_response(asset, !is_index(lookup_key));
    }

    if trimmed.starts_with("api/") {
        return (StatusCode::NOT_FOUND, "not found").into_response();
    }

    if let Some(index) = lookup("index.html") {
        return ok_response(index, false);
    }

    (
        StatusCode::SERVICE_UNAVAILABLE,
        "console assets not built — run `make build-web` first",
    )
        .into_response()
}

fn is_index(path: &str) -> bool {
    path == "index.html"
}

fn ok_response(asset: AssetFile, immutable: bool) -> Response {
    let cache_control = if immutable {
        "public, max-age=31536000, immutable"
    } else {
        "no-cache"
    };
    (
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, asset.mime.into_owned()),
            (header::CACHE_CONTROL, cache_control.to_owned()),
        ],
        asset.bytes.into_owned(),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use axum::body::to_bytes;

    use super::*;

    fn lookup_with(
        map: HashMap<&'static str, (&'static [u8], &'static str)>,
    ) -> impl Fn(&str) -> Option<AssetFile> {
        move |path| {
            map.get(path).map(|(bytes, mime)| AssetFile {
                bytes: Cow::Borrowed(*bytes),
                mime: Cow::Borrowed(*mime),
            })
        }
    }

    async fn body_bytes(response: Response) -> Vec<u8> {
        to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap()
            .to_vec()
    }

    #[tokio::test]
    async fn known_asset_returns_200_with_mime_and_immutable_cache() {
        let map = HashMap::from([(
            "assets/index-abc.js",
            (b"console.log(1)" as &[u8], "application/javascript"),
        )]);
        let response = resolve_asset("/assets/index-abc.js", lookup_with(map));

        assert_eq!(response.status(), StatusCode::OK);
        let headers = response.headers().clone();
        assert_eq!(
            headers.get(header::CONTENT_TYPE).unwrap(),
            "application/javascript"
        );
        assert_eq!(
            headers.get(header::CACHE_CONTROL).unwrap(),
            "public, max-age=31536000, immutable"
        );
        assert_eq!(body_bytes(response).await, b"console.log(1)".to_vec());
    }

    #[tokio::test]
    async fn unknown_non_api_path_falls_back_to_index_html_with_no_cache() {
        let map = HashMap::from([(
            "index.html",
            (b"<!doctype html><div id=app></div>" as &[u8], "text/html"),
        )]);
        let response = resolve_asset("/imports/123", lookup_with(map));

        assert_eq!(response.status(), StatusCode::OK);
        let headers = response.headers().clone();
        assert_eq!(headers.get(header::CONTENT_TYPE).unwrap(), "text/html");
        assert_eq!(headers.get(header::CACHE_CONTROL).unwrap(), "no-cache");
        assert_eq!(
            body_bytes(response).await,
            b"<!doctype html><div id=app></div>".to_vec()
        );
    }

    #[tokio::test]
    async fn unknown_api_path_returns_404_without_serving_index_html() {
        let map = HashMap::from([("index.html", (b"<!doctype html>" as &[u8], "text/html"))]);
        let response = resolve_asset("/api/imports/does-not-exist", lookup_with(map));

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn missing_index_html_returns_503_with_build_hint() {
        let empty: HashMap<&'static str, (&'static [u8], &'static str)> = HashMap::new();
        let response = resolve_asset("/anything", lookup_with(empty));

        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        let body = body_bytes(response).await;
        let text = String::from_utf8(body).unwrap();
        assert!(text.contains("make build-web"), "body was: {text}");
    }

    #[tokio::test]
    async fn root_path_serves_index_html_with_no_cache() {
        let map = HashMap::from([("index.html", (b"<html />" as &[u8], "text/html"))]);
        let response = resolve_asset("/", lookup_with(map));

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers().get(header::CACHE_CONTROL).unwrap(),
            "no-cache"
        );
    }
}
