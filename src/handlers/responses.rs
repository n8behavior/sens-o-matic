use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use uuid::Uuid;
use validator::Validate;

use crate::models::{AppError, AppJson, CreateResponseRequest, Response, UpdateResponseRequest};
use crate::state::AppState;
use crate::state_machine::StateMachine;

#[utoipa::path(
    post,
    path = "/api/pings/{id}/responses",
    params(
        ("id" = Uuid, Path, description = "Ping ID")
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
    Path(ping_id): Path<Uuid>,
    AppJson(request): AppJson<CreateResponseRequest>,
) -> Result<impl IntoResponse, AppError> {
    request.validate()?;
    request.validate_preferences()?;

    let ping = state
        .pings
        .get(&ping_id)
        .ok_or_else(|| AppError::NotFound("Ping".to_string()))?;

    // Check state allows adding responses
    StateMachine::can_add_response(&ping)?;

    // Check for duplicate response
    if ping.has_user_responded(request.user) {
        return Err(AppError::Conflict(
            "User has already responded to this ping".to_string(),
        ));
    }

    // Verify user is a member of the group
    let group = state
        .groups
        .get(&ping.group)
        .ok_or_else(|| AppError::NotFound("Group".to_string()))?;

    if !group.is_member(request.user) {
        return Err(AppError::Forbidden(
            "User is not a member of the group".to_string(),
        ));
    }

    let response = Response::new(request);
    let response_clone = response.clone();

    state
        .pings
        .update(&ping_id, |p| p.add_response(response_clone));

    Ok((StatusCode::CREATED, Json(response)))
}

#[utoipa::path(
    put,
    path = "/api/pings/{id}/responses/{response_id}",
    params(
        ("id" = Uuid, Path, description = "Ping ID"),
        ("response_id" = Uuid, Path, description = "Response ID")
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
    Path((ping_id, response_id)): Path<(Uuid, Uuid)>,
    AppJson(request): AppJson<UpdateResponseRequest>,
) -> Result<Json<Response>, AppError> {
    let ping = state
        .pings
        .get(&ping_id)
        .ok_or_else(|| AppError::NotFound("Ping".to_string()))?;

    // Check state allows modifying responses
    StateMachine::can_add_response(&ping)?;

    // Find the response
    let existing_response = ping
        .responses
        .iter()
        .find(|r| r.id == response_id)
        .ok_or_else(|| AppError::NotFound("Response".to_string()))?;

    // Verify the user owns this response
    if existing_response.user != request.user {
        return Err(AppError::Forbidden(
            "User can only update their own response".to_string(),
        ));
    }

    // Update the response
    let mut updated_response = None;
    state.pings.update(&ping_id, |p| {
        if let Some(r) = p.find_response_mut(response_id) {
            r.update(request.clone());
            updated_response = Some(r.clone());
        }
    });

    updated_response
        .map(Json)
        .ok_or_else(|| AppError::NotFound("Response".to_string()))
}
