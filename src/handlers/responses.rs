//! The endpoints for the answers to a ping: give one and change one.
//!
//! A member answers once, and can change the answer while the ping still takes
//! answers (see [`AnyPing::add_response`](crate::models::AnyPing::add_response)). The answers are stored in the ping, not on
//! their own, so the endpoints work on the ping of the path.
//!
//! The check of the state and the change run together, under the lock of the store
//! (see [`crate::models::ping`]). The group is checked before that, so a member who is not
//! in the group is refused with `403` whatever the state of the ping.

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
};

use crate::models::{
    AppError, AppJson, CreateResponseRequest, PingId, Response, ResponseId, UpdateResponseRequest,
};
use crate::state::AppState;

/// Answers a ping, yes or no.
///
/// A member answers a ping once, and only while the ping is in the `ping_sent` or
/// `gathering` state; the first answer moves it to `gathering`. A yes should come
/// with an `availability`, because the matching ignores a yes without one. The
/// `latest` of the availability must be after its `earliest`, and `max_distance`
/// must not be negative.
#[utoipa::path(
    post,
    path = "/api/pings/{id}/responses",
    params(
        ("id" = inline(PingId), Path, description = "Ping ID")
    ),
    request_body = CreateResponseRequest,
    responses(
        (status = 201, description = "Response submitted successfully", body = Response),
        (status = 400, description = "Invalid request data", body = crate::models::ApiError),
        (status = 403, description = "User not a member of the group", body = crate::models::ApiError),
        (status = 404, description = "Ping not found", body = crate::models::ApiError),
        (status = 409, description = "User already responded to this ping", body = crate::models::ApiError)
    ),
    tag = "Responses"
)]
pub async fn create_response(
    State(state): State<AppState>,
    Path(ping_id): Path<PingId>,
    AppJson(request): AppJson<CreateResponseRequest>,
) -> Result<impl IntoResponse, AppError> {
    // The group of a ping never changes, so it can be read before the update.
    let group_id = state
        .pings
        .get(&ping_id)
        .ok_or_else(|| AppError::NotFound("Ping".to_string()))?
        .group();

    // Verify user is a member of the group
    let group = state
        .groups
        .get(&group_id)
        .ok_or_else(|| AppError::NotFound("Group".to_string()))?;

    if !group.is_member(request.user) {
        return Err(AppError::Forbidden(
            "User is not a member of the group".to_string(),
        ));
    }

    let response = Response::new(request);

    // The state, the duplicate and the change are one step, under the lock.
    state
        .pings
        .update(&ping_id, |ping| ping.add_response(response.clone()))
        .ok_or_else(|| AppError::NotFound("Ping".to_string()))??;

    Ok((StatusCode::CREATED, Json(response)))
}

/// Changes an answer.
///
/// Only the member who gave the answer can change it, and only while the ping is in
/// the `ping_sent` or `gathering` state. Only the fields that are sent change. The
/// new values follow the rules of a new answer: `latest` must be after `earliest`,
/// and `max_distance` must not be negative.
#[utoipa::path(
    put,
    path = "/api/pings/{id}/responses/{response_id}",
    params(
        ("id" = inline(PingId), Path, description = "Ping ID"),
        ("response_id" = inline(ResponseId), Path, description = "Response ID")
    ),
    request_body = UpdateResponseRequest,
    responses(
        (status = 200, description = "Response updated successfully", body = Response),
        (status = 400, description = "Invalid request data", body = crate::models::ApiError),
        (status = 403, description = "User can only update their own response", body = crate::models::ApiError),
        (status = 404, description = "Response not found", body = crate::models::ApiError)
    ),
    tag = "Responses"
)]
pub async fn update_response(
    State(state): State<AppState>,
    Path((ping_id, response_id)): Path<(PingId, ResponseId)>,
    AppJson(request): AppJson<UpdateResponseRequest>,
) -> Result<Json<Response>, AppError> {
    // The state, the owner and the change are one step, under the lock.
    let updated = state
        .pings
        .update(&ping_id, |ping| ping.update_response(response_id, request))
        .ok_or_else(|| AppError::NotFound("Ping".to_string()))??;

    Ok(Json(updated))
}
