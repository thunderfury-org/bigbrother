use axum::{body::Body, http::Request, response::Response};
use std::task::{Context, Poll};
use std::time::Instant;
use tower::{Layer, Service};
use trace_id::TraceId;
use tracing::{Instrument, info, info_span};

#[derive(Clone)]
pub struct LogLayer;

impl<S> Layer<S> for LogLayer {
    type Service = LogMiddleware<S>;

    fn layer(&self, inner: S) -> Self::Service {
        LogMiddleware { inner }
    }
}

#[derive(Clone)]
pub struct LogMiddleware<S> {
    inner: S,
}

impl<S> Service<Request<Body>> for LogMiddleware<S>
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
        let method = req.method().clone();
        let uri = req.uri().clone();

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

        let trace_id = req.extensions().get::<TraceId>().cloned().unwrap_or_default();
        let span = info_span!(
            "request",
            trace_id = %trace_id.as_str()
        );

        let start = Instant::now();
        let future = self.inner.call(req);

        Box::pin(
            async move {
                let response = future.await?;

                let status = response.status();
                let latency_ms = start.elapsed().as_millis();
                info!(
                    "{} \"{}\" {} \"{}\" \"{}\" {}ms",
                    method,
                    uri,
                    status.as_u16(),
                    referer,
                    user_agent,
                    latency_ms
                );

                Ok(response)
            }
            .instrument(span),
        )
    }
}
