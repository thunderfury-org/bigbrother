use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};

use crate::{
    application::{ports::SubscriptionCreateInput, subscription::manage::SubscriptionCandidate},
    domain::subscription::SubscriptionMediaType,
};

use super::console::{ConsoleContext, app_error_to_response, json_response};

#[derive(Deserialize)]
pub(super) struct CandidatesQuery {
    query: String,
}

#[derive(Deserialize)]
pub(super) struct CreateSubscriptionRequest {
    tmdb_id: u32,
    media_type: String,
    title_zh: Option<String>,
    title_en: Option<String>,
}

#[derive(Serialize)]
struct SubscriptionItem {
    id: i64,
    tmdb_id: u32,
    media_type: String,
    title_zh: Option<String>,
    title_en: Option<String>,
    create_time: String,
    update_time: String,
}

#[derive(Serialize)]
struct CandidateItem {
    tmdb_id: u32,
    media_type: String,
    title: String,
    original_title: String,
}

#[derive(Serialize)]
struct SubscriptionsListResponse {
    items: Vec<SubscriptionItem>,
}

#[derive(Serialize)]
struct CandidatesResponse {
    candidates: Vec<CandidateItem>,
}

#[derive(Serialize)]
struct CreateSubscriptionResponse {
    id: i64,
}

pub(super) async fn list_subscriptions(State(ctx): State<ConsoleContext>) -> Response {
    let Some(svc) = ctx.subscription_service.as_ref() else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            "subscription service not available",
        )
            .into_response();
    };
    match svc.list().await {
        Ok(records) => {
            let items: Vec<SubscriptionItem> = records
                .into_iter()
                .map(|r| SubscriptionItem {
                    id: r.id,
                    tmdb_id: r.tmdb_id,
                    media_type: r.media_type.as_str().to_owned(),
                    title_zh: r.title_zh,
                    title_en: r.title_en,
                    create_time: r.create_time.to_rfc3339(),
                    update_time: r.update_time.to_rfc3339(),
                })
                .collect();
            json_response(StatusCode::OK, &SubscriptionsListResponse { items })
        }
        Err(err) => app_error_to_response(err),
    }
}

pub(super) async fn search_candidates(
    State(ctx): State<ConsoleContext>,
    Query(query): Query<CandidatesQuery>,
) -> Response {
    let Some(svc) = ctx.subscription_service.as_ref() else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            "subscription service not available",
        )
            .into_response();
    };
    match svc.candidates(&query.query).await {
        Ok(candidates) => {
            let items: Vec<CandidateItem> = candidates.into_iter().map(candidate_to_json).collect();
            json_response(StatusCode::OK, &CandidatesResponse { candidates: items })
        }
        Err(err) => app_error_to_response(err),
    }
}

fn candidate_to_json(c: SubscriptionCandidate) -> CandidateItem {
    CandidateItem {
        tmdb_id: c.tmdb_id,
        media_type: c.media_type.as_str().to_owned(),
        title: c.title,
        original_title: c.original_title,
    }
}

pub(super) async fn create_subscription(
    State(ctx): State<ConsoleContext>,
    axum::Json(body): axum::Json<CreateSubscriptionRequest>,
) -> Response {
    let Some(svc) = ctx.subscription_service.as_ref() else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            "subscription service not available",
        )
            .into_response();
    };
    let media_type = match SubscriptionMediaType::from_str(&body.media_type) {
        Some(mt) => mt,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                format!("invalid media_type: {}", body.media_type),
            )
                .into_response();
        }
    };
    let input = SubscriptionCreateInput {
        tmdb_id: body.tmdb_id,
        media_type,
        title_zh: body.title_zh,
        title_en: body.title_en,
    };
    match svc.create(input).await {
        Ok(id) => json_response(StatusCode::CREATED, &CreateSubscriptionResponse { id }),
        Err(err) => app_error_to_response(err),
    }
}

pub(super) async fn delete_subscription(
    State(ctx): State<ConsoleContext>,
    Path(id): Path<i64>,
) -> Response {
    let Some(svc) = ctx.subscription_service.as_ref() else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            "subscription service not available",
        )
            .into_response();
    };
    match svc.delete(id).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(err) => app_error_to_response(err),
    }
}

pub(super) async fn rescan_subscription(
    State(ctx): State<ConsoleContext>,
    Path(id): Path<i64>,
) -> Response {
    let Some((((import_service, identify_service), recorded_import), sub_repo)) = ctx
        .import_service
        .as_ref()
        .zip(ctx.identify_service.as_ref())
        .zip(ctx.recorded_import.as_ref())
        .zip(ctx.subscription_repo.as_ref())
    else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            "import service not available",
        )
            .into_response();
    };

    let identifier = identify_service.as_ref().clone();
    let importer = import_service.as_ref().clone();

    match crate::application::subscription::rescan::rescan_subscription(
        id,
        sub_repo.as_ref(),
        ctx.file_index_service.as_ref(),
        &identifier,
        &importer,
        recorded_import,
    )
    .await
    {
        Ok(results) => json_response(StatusCode::OK, &results),
        Err(err) => app_error_to_response(err),
    }
}
