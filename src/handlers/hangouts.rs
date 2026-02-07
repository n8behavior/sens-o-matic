use axum::{
    extract::{Path, State},
    Json,
};
use uuid::Uuid;

use crate::models::{AppError, AppJson, Hangout, UpdateAttendeeStatusRequest};
use crate::state::AppState;
use crate::state_machine::StateMachine;

#[utoipa::path(
    get,
    path = "/api/hangouts/{id}",
    params(
        ("id" = Uuid, Path, description = "Hangout ID")
    ),
    responses(
        (status = 200, description = "Hangout found", body = Hangout),
        (status = 404, description = "Hangout not found", body = crate::models::ApiError)
    ),
    tag = "Hangouts"
)]
pub async fn get_hangout(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Hangout>, AppError> {
    state
        .hangouts
        .get(&id)
        .map(Json)
        .ok_or_else(|| AppError::NotFound("Hangout".to_string()))
}

#[utoipa::path(
    post,
    path = "/api/hangouts/{id}/activate",
    params(
        ("id" = Uuid, Path, description = "Hangout ID")
    ),
    responses(
        (status = 200, description = "Hangout activated", body = Hangout),
        (status = 404, description = "Hangout not found", body = crate::models::ApiError),
        (status = 409, description = "Hangout not in confirmed state", body = crate::models::ApiError)
    ),
    tag = "Hangouts"
)]
pub async fn activate_hangout(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Hangout>, AppError> {
    let hangout = state
        .hangouts
        .get(&id)
        .ok_or_else(|| AppError::NotFound("Hangout".to_string()))?;

    StateMachine::can_activate_hangout(&hangout)?;

    let updated = state
        .hangouts
        .update(&id, |h| h.activate())
        .ok_or_else(|| AppError::NotFound("Hangout".to_string()))?;

    // Sync ping state
    StateMachine::sync_ping_state_from_hangout(&state, &updated);

    Ok(Json(updated))
}

#[utoipa::path(
    post,
    path = "/api/hangouts/{id}/complete",
    params(
        ("id" = Uuid, Path, description = "Hangout ID")
    ),
    responses(
        (status = 200, description = "Hangout completed", body = Hangout),
        (status = 404, description = "Hangout not found", body = crate::models::ApiError),
        (status = 409, description = "Hangout not in active state", body = crate::models::ApiError)
    ),
    tag = "Hangouts"
)]
pub async fn complete_hangout(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Hangout>, AppError> {
    let hangout = state
        .hangouts
        .get(&id)
        .ok_or_else(|| AppError::NotFound("Hangout".to_string()))?;

    StateMachine::can_complete_hangout(&hangout)?;

    let updated = state
        .hangouts
        .update(&id, |h| h.complete())
        .ok_or_else(|| AppError::NotFound("Hangout".to_string()))?;

    // Sync ping state
    StateMachine::sync_ping_state_from_hangout(&state, &updated);

    Ok(Json(updated))
}

#[utoipa::path(
    put,
    path = "/api/hangouts/{id}/attendees/{user_id}/status",
    params(
        ("id" = Uuid, Path, description = "Hangout ID"),
        ("user_id" = Uuid, Path, description = "User ID")
    ),
    request_body = UpdateAttendeeStatusRequest,
    responses(
        (status = 200, description = "Attendee status updated", body = Hangout),
        (status = 403, description = "User can only update their own status", body = crate::models::ApiError),
        (status = 404, description = "Hangout not found", body = crate::models::ApiError)
    ),
    tag = "Hangouts"
)]
pub async fn update_attendee_status(
    State(state): State<AppState>,
    Path((hangout_id, user_id)): Path<(Uuid, Uuid)>,
    AppJson(request): AppJson<UpdateAttendeeStatusRequest>,
) -> Result<Json<Hangout>, AppError> {
    let hangout = state
        .hangouts
        .get(&hangout_id)
        .ok_or_else(|| AppError::NotFound("Hangout".to_string()))?;

    // Verify the user is an attendee
    if !hangout.is_attendee(user_id) {
        return Err(AppError::NotFound("Attendee".to_string()));
    }

    let updated = state
        .hangouts
        .update(&hangout_id, |h| {
            h.update_attendee_status(user_id, request.status)
        })
        .ok_or_else(|| AppError::NotFound("Hangout".to_string()))?;

    Ok(Json(updated))
}
