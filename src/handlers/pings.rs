//! The endpoints for pings: send one, read it, and move it through its lifecycle.
//!
//! Each endpoint after the creation of a ping is one transition, or one read, of the
//! lifecycle described in [`crate::models::ping`]. The rules of each transition are
//! those of [`crate::state_machine`], and the matching is that of [`crate::matching`].
//! The answers of the members are the business of [`super::responses`].

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
};
use uuid::Uuid;
use validator::Validate;

use crate::matching::MatchingEngine;
use crate::models::{
    AppError, AppJson, CancelPingRequest, ConfirmHangoutRequest, CreatePingRequest, MatchResults,
    Ping, TriggerMatchRequest, UpdateAttendeeStatusRequest,
};
use crate::state::AppState;
use crate::state_machine::StateMachine;

/// Sends a ping to a group.
///
/// The group must exist and the initiator must be one of its members. The ping starts
/// in the `ping_sent` state.
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

/// Returns a ping, with its state and the data of that state.
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

/// Cancels a ping.
///
/// Only the initiator can, and only while the ping is not in an end state
/// (`complete`, `cancelled` or `no_match`). The answers are kept; a confirmed hangout is
/// dropped.
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

/// Ends the gathering of answers and runs the matching.
///
/// Only the initiator can, and only in the `gathering` state, so at least one member
/// must have answered. The ping moves to `matching` when the members who said yes
/// and gave an availability share a time, and to `no_match` when they do not.
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

    // Transition state with match results embedded
    let updated = state
        .pings
        .update(&id, |p| {
            StateMachine::transition_to_matching(p, match_results)
        })
        .ok_or_else(|| AppError::NotFound("Ping".to_string()))?;

    Ok(Json(updated))
}

/// Returns the match results of a ping.
///
/// In the `matching` state these are the results that the matching stored. In any
/// other state they are worked out from the answers as they are now, and the ping
/// does not change; this works also while the ping is gathering, as a preview.
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

    // Get match results from lifecycle or calculate them
    let match_results = ping
        .lifecycle
        .match_results()
        .cloned()
        .unwrap_or_else(|| MatchingEngine::calculate_match(&ping));

    Ok(Json(match_results))
}

/// Confirms the hangout of a ping that has a match.
///
/// The ping must be in the `matching` state. It moves to `venue_confirmed`. The
/// attendees are all the members who said yes, each one `pending`, and the timeline
/// is stored as it is sent: the server does not compare it with the common window
/// and does not check who confirms.
#[utoipa::path(
    post,
    path = "/api/pings/{id}/confirm",
    params(
        ("id" = Uuid, Path, description = "Ping ID")
    ),
    request_body = ConfirmHangoutRequest,
    responses(
        (status = 201, description = "Hangout confirmed", body = Ping),
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

    // Create hangout data
    let hangout_data = StateMachine::create_hangout_data(&ping, request.timeline);

    // Transition ping state
    let updated = state
        .pings
        .update(&id, |p| {
            StateMachine::transition_to_venue_confirmed(p, hangout_data)
        })
        .ok_or_else(|| AppError::NotFound("Ping".to_string()))?;

    Ok((StatusCode::CREATED, Json(updated)))
}

/// Starts the hangout: moves a ping from `venue_confirmed` to `active_hangout`.
#[utoipa::path(
    post,
    path = "/api/pings/{id}/activate",
    params(
        ("id" = Uuid, Path, description = "Ping ID")
    ),
    responses(
        (status = 200, description = "Ping activated (hangout started)", body = Ping),
        (status = 404, description = "Ping not found", body = crate::models::ApiError),
        (status = 409, description = "Ping not in venue_confirmed state", body = crate::models::ApiError)
    ),
    tag = "Pings"
)]
pub async fn activate_ping(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Ping>, AppError> {
    let ping = state
        .pings
        .get(&id)
        .ok_or_else(|| AppError::NotFound("Ping".to_string()))?;

    StateMachine::can_activate(&ping)?;

    let updated = state
        .pings
        .update(&id, StateMachine::transition_to_active)
        .ok_or_else(|| AppError::NotFound("Ping".to_string()))?;

    Ok(Json(updated))
}

/// Ends the hangout: moves a ping from `active_hangout` to `complete`.
#[utoipa::path(
    post,
    path = "/api/pings/{id}/complete",
    params(
        ("id" = Uuid, Path, description = "Ping ID")
    ),
    responses(
        (status = 200, description = "Ping completed", body = Ping),
        (status = 404, description = "Ping not found", body = crate::models::ApiError),
        (status = 409, description = "Ping not in active_hangout state", body = crate::models::ApiError)
    ),
    tag = "Pings"
)]
pub async fn complete_ping(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Ping>, AppError> {
    let ping = state
        .pings
        .get(&id)
        .ok_or_else(|| AppError::NotFound("Ping".to_string()))?;

    StateMachine::can_complete(&ping)?;

    let updated = state
        .pings
        .update(&id, StateMachine::transition_to_complete)
        .ok_or_else(|| AppError::NotFound("Ping".to_string()))?;

    Ok(Json(updated))
}

/// Sets the status of one attendee of the hangout.
///
/// The ping must have a hangout, that is, be in the `venue_confirmed`,
/// `active_hangout` or `complete` state, and the user must be one of its
/// attendees. A status can follow any other.
#[utoipa::path(
    put,
    path = "/api/pings/{id}/attendees/{user_id}/status",
    params(
        ("id" = Uuid, Path, description = "Ping ID"),
        ("user_id" = Uuid, Path, description = "User ID")
    ),
    request_body = UpdateAttendeeStatusRequest,
    responses(
        (status = 200, description = "Attendee status updated", body = Ping),
        (status = 404, description = "Ping not found or user not an attendee", body = crate::models::ApiError),
        (status = 409, description = "Ping not in active_hangout or venue_confirmed state", body = crate::models::ApiError)
    ),
    tag = "Pings"
)]
pub async fn update_attendee_status(
    State(state): State<AppState>,
    Path((ping_id, user_id)): Path<(Uuid, Uuid)>,
    AppJson(request): AppJson<UpdateAttendeeStatusRequest>,
) -> Result<Json<Ping>, AppError> {
    let ping = state
        .pings
        .get(&ping_id)
        .ok_or_else(|| AppError::NotFound("Ping".to_string()))?;

    // Verify the ping has a hangout and the user is an attendee
    let hangout = ping
        .lifecycle
        .hangout()
        .ok_or_else(|| AppError::Conflict("Ping does not have an active hangout".to_string()))?;

    if !hangout.is_attendee(user_id) {
        return Err(AppError::NotFound("Attendee".to_string()));
    }

    let status = request.status;
    let updated = state
        .pings
        .update(&ping_id, |p| {
            if let Some(h) = p.lifecycle.hangout_mut() {
                h.update_attendee_status(user_id, status);
            }
        })
        .ok_or_else(|| AppError::NotFound("Ping".to_string()))?;

    Ok(Json(updated))
}
