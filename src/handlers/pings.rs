//! The endpoints for pings: send one, read it, and move it through its lifecycle.
//!
//! Each endpoint after the creation of a ping is one transition, or one read, of the
//! lifecycle described in [`crate::models::ping`], which also has the rules of each
//! transition. The matching is that of [`crate::matching`]. The answers of the members are
//! the business of [`super::responses`].
//!
//! A transition is one call of a method of [`AnyPing`] inside the closure of
//! [`InMemoryStore::update`](crate::state::InMemoryStore::update), so that the check of
//! the state and the change run under the lock of the store, as one step.

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
};

use crate::matching::MatchingEngine;
use crate::models::{
    AnyPing, AppError, AppJson, CancelPingRequest, ConfirmHangoutRequest, CreatePingRequest,
    MatchResults, Ping, PingId, TriggerMatchRequest, UpdateAttendeeStatusRequest, UserId,
};
use crate::state::AppState;

/// Runs `action` on the ping with this id and returns the ping as it is after.
///
/// `action` runs under the lock of the store, so the check of the state that it
/// makes and the change that follows are one step, and the ping that it returns
/// is the one that it changed. If `action` fails, the ping is as it was.
///
/// # Errors
///
/// Returns [`AppError::NotFound`] when there is no such ping, and otherwise the
/// error of `action`.
pub(super) fn transition(
    state: &AppState,
    id: PingId,
    action: impl FnOnce(&mut AnyPing) -> Result<(), AppError>,
) -> Result<AnyPing, AppError> {
    state
        .pings
        .update(&id, |ping| action(ping).map(|()| ping.clone()))
        .ok_or_else(|| AppError::NotFound("Ping".to_string()))?
}

/// Sends a ping to a group.
///
/// The group must exist and the initiator must be one of its members. The ping starts
/// in the `ping_sent` state.
#[utoipa::path(
    post,
    path = "/api/pings",
    request_body = CreatePingRequest,
    responses(
        (status = 201, description = "Ping created successfully", body = AnyPing),
        (status = 400, description = "Invalid request data", body = crate::models::ApiError),
        (status = 403, description = "User not a member of the group", body = crate::models::ApiError)
    ),
    tag = "Pings"
)]
pub async fn create_ping(
    State(state): State<AppState>,
    AppJson(request): AppJson<CreatePingRequest>,
) -> Result<impl IntoResponse, AppError> {
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

    let ping = AnyPing::from(Ping::new(request));
    state.pings.insert(ping.id(), ping.clone());

    Ok((StatusCode::CREATED, Json(ping)))
}

/// Returns a ping, with its state and the data of that state.
#[utoipa::path(
    get,
    path = "/api/pings/{id}",
    params(
        ("id" = inline(PingId), Path, description = "Ping ID")
    ),
    responses(
        (status = 200, description = "Ping found", body = AnyPing),
        (status = 404, description = "Ping not found", body = crate::models::ApiError)
    ),
    tag = "Pings"
)]
pub async fn get_ping(
    State(state): State<AppState>,
    Path(id): Path<PingId>,
) -> Result<Json<AnyPing>, AppError> {
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
        ("id" = inline(PingId), Path, description = "Ping ID")
    ),
    request_body = CancelPingRequest,
    responses(
        (status = 200, description = "Ping cancelled successfully", body = AnyPing),
        (status = 403, description = "Only initiator can cancel", body = crate::models::ApiError),
        (status = 404, description = "Ping not found", body = crate::models::ApiError),
        (status = 409, description = "Ping already in terminal state", body = crate::models::ApiError)
    ),
    tag = "Pings"
)]
pub async fn cancel_ping(
    State(state): State<AppState>,
    Path(id): Path<PingId>,
    AppJson(request): AppJson<CancelPingRequest>,
) -> Result<Json<AnyPing>, AppError> {
    transition(&state, id, |ping| ping.cancel(request.user_id)).map(Json)
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
        ("id" = inline(PingId), Path, description = "Ping ID")
    ),
    request_body = TriggerMatchRequest,
    responses(
        (status = 200, description = "Matching triggered successfully", body = AnyPing),
        (status = 403, description = "Only initiator can trigger matching", body = crate::models::ApiError),
        (status = 404, description = "Ping not found", body = crate::models::ApiError),
        (status = 409, description = "Ping not in gathering state", body = crate::models::ApiError)
    ),
    tag = "Pings"
)]
pub async fn trigger_match(
    State(state): State<AppState>,
    Path(id): Path<PingId>,
    AppJson(request): AppJson<TriggerMatchRequest>,
) -> Result<Json<AnyPing>, AppError> {
    transition(&state, id, |ping| {
        ping.trigger_match(request.user_id, MatchingEngine::calculate_match)
    })
    .map(Json)
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
        ("id" = inline(PingId), Path, description = "Ping ID")
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
    Path(id): Path<PingId>,
) -> Result<Json<MatchResults>, AppError> {
    let ping = state
        .pings
        .get(&id)
        .ok_or_else(|| AppError::NotFound("Ping".to_string()))?;

    // Get match results from the state or calculate them
    let match_results = ping
        .match_results()
        .cloned()
        .unwrap_or_else(|| MatchingEngine::calculate_match(ping.id(), ping.responses()));

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
        ("id" = inline(PingId), Path, description = "Ping ID")
    ),
    request_body = ConfirmHangoutRequest,
    responses(
        (status = 201, description = "Hangout confirmed", body = AnyPing),
        (status = 400, description = "Invalid request data", body = crate::models::ApiError),
        (status = 404, description = "Ping not found", body = crate::models::ApiError),
        (status = 409, description = "Ping not in matching state", body = crate::models::ApiError)
    ),
    tag = "Pings"
)]
pub async fn confirm_hangout(
    State(state): State<AppState>,
    Path(id): Path<PingId>,
    AppJson(request): AppJson<ConfirmHangoutRequest>,
) -> Result<impl IntoResponse, AppError> {
    let updated = transition(&state, id, |ping| ping.confirm(request.timeline))?;

    Ok((StatusCode::CREATED, Json(updated)))
}

/// Starts the hangout: moves a ping from `venue_confirmed` to `active_hangout`.
#[utoipa::path(
    post,
    path = "/api/pings/{id}/activate",
    params(
        ("id" = inline(PingId), Path, description = "Ping ID")
    ),
    responses(
        (status = 200, description = "Ping activated (hangout started)", body = AnyPing),
        (status = 404, description = "Ping not found", body = crate::models::ApiError),
        (status = 409, description = "Ping not in venue_confirmed state", body = crate::models::ApiError)
    ),
    tag = "Pings"
)]
pub async fn activate_ping(
    State(state): State<AppState>,
    Path(id): Path<PingId>,
) -> Result<Json<AnyPing>, AppError> {
    transition(&state, id, AnyPing::activate).map(Json)
}

/// Ends the hangout: moves a ping from `active_hangout` to `complete`.
#[utoipa::path(
    post,
    path = "/api/pings/{id}/complete",
    params(
        ("id" = inline(PingId), Path, description = "Ping ID")
    ),
    responses(
        (status = 200, description = "Ping completed", body = AnyPing),
        (status = 404, description = "Ping not found", body = crate::models::ApiError),
        (status = 409, description = "Ping not in active_hangout state", body = crate::models::ApiError)
    ),
    tag = "Pings"
)]
pub async fn complete_ping(
    State(state): State<AppState>,
    Path(id): Path<PingId>,
) -> Result<Json<AnyPing>, AppError> {
    transition(&state, id, AnyPing::complete).map(Json)
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
        ("id" = inline(PingId), Path, description = "Ping ID"),
        ("user_id" = inline(UserId), Path, description = "User ID")
    ),
    request_body = UpdateAttendeeStatusRequest,
    responses(
        (status = 200, description = "Attendee status updated", body = AnyPing),
        (status = 404, description = "Ping not found or user not an attendee", body = crate::models::ApiError),
        (status = 409, description = "Ping not in active_hangout or venue_confirmed state", body = crate::models::ApiError)
    ),
    tag = "Pings"
)]
pub async fn update_attendee_status(
    State(state): State<AppState>,
    Path((ping_id, user_id)): Path<(PingId, UserId)>,
    AppJson(request): AppJson<UpdateAttendeeStatusRequest>,
) -> Result<Json<AnyPing>, AppError> {
    transition(&state, ping_id, |ping| {
        ping.set_attendee_status(user_id, request.status)
    })
    .map(Json)
}
