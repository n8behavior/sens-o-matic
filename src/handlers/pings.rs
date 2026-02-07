use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use uuid::Uuid;
use validator::Validate;

use crate::matching::MatchingEngine;
use crate::models::{
    AppError, AppJson, CancelPingRequest, ConfirmHangoutRequest, CreatePingRequest, Hangout,
    MatchResults, Ping, TriggerMatchRequest,
};
use crate::state::AppState;
use crate::state_machine::StateMachine;

#[utoipa::path(
    post,
    path = "/api/pings",
    request_body = CreatePingRequest,
    responses(
        (status = 201, description = "Ping created successfully", body = Ping),
        (status = 400, description = "Invalid request data", body = crate::models::ApiError),
        (status = 403, description = "User not a member of the group", body = crate::models::ApiError)
    ),
    tag = "Pings"
)]
pub async fn create_ping(
    State(state): State<AppState>,
    AppJson(request): AppJson<CreatePingRequest>,
) -> Result<impl IntoResponse, AppError> {
    request.validate()?;

    // Verify user is a member of the group
    let group = state
        .groups
        .get(&request.group)
        .ok_or_else(|| AppError::NotFound("Group".to_string()))?;

    if !group.is_member(request.initiator) {
        return Err(AppError::Forbidden(
            "User is not a member of the group".to_string(),
        ));
    }

    let ping = Ping::new(request);
    state.pings.insert(ping.id, ping.clone());

    Ok((StatusCode::CREATED, Json(ping)))
}

#[utoipa::path(
    get,
    path = "/api/pings/{id}",
    params(
        ("id" = Uuid, Path, description = "Ping ID")
    ),
    responses(
        (status = 200, description = "Ping found", body = Ping),
        (status = 404, description = "Ping not found", body = crate::models::ApiError)
    ),
    tag = "Pings"
)]
pub async fn get_ping(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Ping>, AppError> {
    state
        .pings
        .get(&id)
        .map(Json)
        .ok_or_else(|| AppError::NotFound("Ping".to_string()))
}

#[utoipa::path(
    post,
    path = "/api/pings/{id}/cancel",
    params(
        ("id" = Uuid, Path, description = "Ping ID")
    ),
    request_body = CancelPingRequest,
    responses(
        (status = 200, description = "Ping cancelled successfully", body = Ping),
        (status = 403, description = "Only initiator can cancel", body = crate::models::ApiError),
        (status = 404, description = "Ping not found", body = crate::models::ApiError),
        (status = 409, description = "Ping already in terminal state", body = crate::models::ApiError)
    ),
    tag = "Pings"
)]
pub async fn cancel_ping(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    AppJson(request): AppJson<CancelPingRequest>,
) -> Result<Json<Ping>, AppError> {
    let ping = state
        .pings
        .get(&id)
        .ok_or_else(|| AppError::NotFound("Ping".to_string()))?;

    StateMachine::can_cancel(&ping, request.user_id)?;

    let updated = state
        .pings
        .update(&id, StateMachine::transition_to_cancelled)
        .ok_or_else(|| AppError::NotFound("Ping".to_string()))?;

    Ok(Json(updated))
}

#[utoipa::path(
    post,
    path = "/api/pings/{id}/match",
    params(
        ("id" = Uuid, Path, description = "Ping ID")
    ),
    request_body = TriggerMatchRequest,
    responses(
        (status = 200, description = "Matching triggered successfully", body = Ping),
        (status = 403, description = "Only initiator can trigger matching", body = crate::models::ApiError),
        (status = 404, description = "Ping not found", body = crate::models::ApiError),
        (status = 409, description = "Ping not in gathering state", body = crate::models::ApiError)
    ),
    tag = "Pings"
)]
pub async fn trigger_match(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    AppJson(request): AppJson<TriggerMatchRequest>,
) -> Result<Json<Ping>, AppError> {
    let ping = state
        .pings
        .get(&id)
        .ok_or_else(|| AppError::NotFound("Ping".to_string()))?;

    StateMachine::can_trigger_match(&ping, request.user_id)?;

    // Calculate match results
    let match_results = MatchingEngine::calculate_match(&ping);
    let has_match = match_results.has_match;

    // Store match results
    state.match_results.insert(ping.id, match_results);

    // Transition state
    let updated = state
        .pings
        .update(&id, |p| StateMachine::transition_to_matching(p, has_match))
        .ok_or_else(|| AppError::NotFound("Ping".to_string()))?;

    Ok(Json(updated))
}

#[utoipa::path(
    get,
    path = "/api/pings/{id}/match-results",
    params(
        ("id" = Uuid, Path, description = "Ping ID")
    ),
    responses(
        (status = 200, description = "Match results", body = MatchResults),
        (status = 404, description = "Ping not found", body = crate::models::ApiError),
        (status = 409, description = "Ping not in matching or later state", body = crate::models::ApiError)
    ),
    tag = "Pings"
)]
pub async fn get_match_results(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<MatchResults>, AppError> {
    let ping = state
        .pings
        .get(&id)
        .ok_or_else(|| AppError::NotFound("Ping".to_string()))?;

    // Check if ping has been through matching
    let results = state.match_results.get(&ping.id);
    let match_results = MatchingEngine::get_or_calculate_match(&ping, results);

    Ok(Json(match_results))
}

#[utoipa::path(
    post,
    path = "/api/pings/{id}/confirm",
    params(
        ("id" = Uuid, Path, description = "Ping ID")
    ),
    request_body = ConfirmHangoutRequest,
    responses(
        (status = 201, description = "Hangout confirmed", body = Hangout),
        (status = 400, description = "Invalid request data", body = crate::models::ApiError),
        (status = 404, description = "Ping not found", body = crate::models::ApiError),
        (status = 409, description = "Ping not in matching state", body = crate::models::ApiError)
    ),
    tag = "Pings"
)]
pub async fn confirm_hangout(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    AppJson(request): AppJson<ConfirmHangoutRequest>,
) -> Result<impl IntoResponse, AppError> {
    let ping = state
        .pings
        .get(&id)
        .ok_or_else(|| AppError::NotFound("Ping".to_string()))?;

    StateMachine::can_confirm(&ping)?;

    // Create hangout
    let hangout = StateMachine::create_hangout(&ping, request.timeline);
    let hangout_id = hangout.id;
    state.hangouts.insert(hangout.id, hangout.clone());

    // Transition ping state
    state
        .pings
        .update(&id, |p| StateMachine::transition_to_venue_confirmed(p, hangout_id));

    Ok((StatusCode::CREATED, Json(hangout)))
}
