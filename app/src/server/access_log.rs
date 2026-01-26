use axum::{body::Body, extract::ConnectInfo, http::Request, response::Response};
use std::{
    net::SocketAddr,
    task::{Context, Poll},
};
use tower::{Layer, Service};
use tracing::info;

/// Custom layer for nginx-style access logging
#[derive(Clone)]
pub struct AccessLogLayer;

impl<S> Layer<S> for AccessLogLayer {
    type Service = AccessLogMiddleware<S>;

    fn layer(&self, inner: S) -> Self::Service {
        AccessLogMiddleware { inner }
    }
}

#[derive(Clone)]
pub struct AccessLogMiddleware<S> {
    inner: S,
}

impl<S> Service<Request<Body>> for AccessLogMiddleware<S>
where
    S: Service<Request<Body>, Response = Response> + Clone + Send + 'static,
    S::Future: Send + 'static,
{
    type Response = Response;
    type Error = S::Error;
    type Future = std::pin::Pin<Box<dyn std::future::Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: Request<Body>) -> Self::Future {
        let mut inner = self.inner.clone();
        let method = req.method().clone();
        let uri = req.uri().clone();

        let remote_addr = req
            .extensions()
            .get::<ConnectInfo<SocketAddr>>()
            .map(|ConnectInfo(addr)| addr.to_string())
            .unwrap_or_else(|| "-".to_string());

        let user_agent = req
            .headers()
            .get("user-agent")
            .and_then(|h| h.to_str().ok())
            .map(|s| s.to_string())
            .unwrap_or_else(|| "-".to_string());

        let referer = req
            .headers()
            .get("referer")
            .and_then(|h| h.to_str().ok())
            .map(|s| s.to_string())
            .unwrap_or_else(|| "-".to_string());

        Box::pin(async move {
            let response = inner.call(req).await?;

            let status = response.status();

            info!(
                target: "media_server",
                "{} {} {} {} 0 \"{}\" \"{}\"",
                remote_addr,
                method,
                uri,
                status.as_u16(),
                referer,
                user_agent
            );

            Ok(response)
        })
    }
}
