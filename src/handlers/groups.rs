use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::Deserialize;
use uuid::Uuid;
use validator::Validate;

use crate::models::{
    AppError, AppJson, CreateGroupRequest, Group, JoinGroupRequest, LeaveGroupRequest, Ping,
    PingState, RegenerateInviteRequest,
};
use crate::state::AppState;

#[utoipa::path(
    post,
    path = "/api/groups",
    request_body = CreateGroupRequest,
    responses(
        (status = 201, description = "Group created successfully", body = Group),
        (status = 400, description = "Invalid request data", body = crate::models::ApiError)
    ),
    tag = "Groups"
)]
pub async fn create_group(
    State(state): State<AppState>,
    AppJson(request): AppJson<CreateGroupRequest>,
) -> Result<impl IntoResponse, AppError> {
    request.validate()?;

    let group = Group::new(request);
    state.groups.insert(group.id, group.clone());

    Ok((StatusCode::CREATED, Json(group)))
}

#[utoipa::path(
    get,
    path = "/api/groups/{id}",
    params(
        ("id" = Uuid, Path, description = "Group ID")
    ),
    responses(
        (status = 200, description = "Group found", body = Group),
        (status = 404, description = "Group not found", body = crate::models::ApiError)
    ),
    tag = "Groups"
)]
pub async fn get_group(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Group>, AppError> {
    state
        .groups
        .get(&id)
        .map(Json)
        .ok_or_else(|| AppError::NotFound("Group".to_string()))
}

#[utoipa::path(
    post,
    path = "/api/groups/join",
    request_body = JoinGroupRequest,
    responses(
        (status = 200, description = "Successfully joined group", body = Group),
        (status = 400, description = "Invalid request data", body = crate::models::ApiError),
        (status = 404, description = "Invalid invite code", body = crate::models::ApiError)
    ),
    tag = "Groups"
)]
pub async fn join_group(
    State(state): State<AppState>,
    AppJson(request): AppJson<JoinGroupRequest>,
) -> Result<Json<Group>, AppError> {
    request.validate()?;

    let group = state
        .find_group_by_invite_code(&request.invite_code)
        .ok_or_else(|| AppError::NotFound("Group".to_string()))?;

    let updated = state
        .groups
        .update(&group.id, |g| g.add_member(request.user_id))
        .ok_or_else(|| AppError::NotFound("Group".to_string()))?;

    Ok(Json(updated))
}

#[utoipa::path(
    post,
    path = "/api/groups/{id}/leave",
    params(
        ("id" = Uuid, Path, description = "Group ID")
    ),
    request_body = LeaveGroupRequest,
    responses(
        (status = 204, description = "Successfully left group"),
        (status = 404, description = "Group not found", body = crate::models::ApiError)
    ),
    tag = "Groups"
)]
pub async fn leave_group(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    AppJson(request): AppJson<LeaveGroupRequest>,
) -> Result<StatusCode, AppError> {
    state
        .groups
        .update(&id, |g| g.remove_member(request.user_id))
        .ok_or_else(|| AppError::NotFound("Group".to_string()))?;

    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    post,
    path = "/api/groups/{id}/regenerate-invite",
    params(
        ("id" = Uuid, Path, description = "Group ID")
    ),
    request_body = RegenerateInviteRequest,
    responses(
        (status = 200, description = "Invite code regenerated", body = Group),
        (status = 403, description = "Access denied", body = crate::models::ApiError),
        (status = 404, description = "Group not found", body = crate::models::ApiError)
    ),
    tag = "Groups"
)]
pub async fn regenerate_invite_code(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    AppJson(request): AppJson<RegenerateInviteRequest>,
) -> Result<Json<Group>, AppError> {
    let group = state
        .groups
        .get(&id)
        .ok_or_else(|| AppError::NotFound("Group".to_string()))?;

    // Check if user is a member (for now, any member can regenerate)
    if !group.is_member(request.user_id) {
        return Err(AppError::Forbidden(
            "Only group members can regenerate invite code".to_string(),
        ));
    }

    let updated = state
        .groups
        .update(&id, |g| g.regenerate_invite_code())
        .ok_or_else(|| AppError::NotFound("Group".to_string()))?;

    Ok(Json(updated))
}

#[derive(Debug, Deserialize)]
pub struct ListPingsQuery {
    pub state: Option<PingState>,
}

#[utoipa::path(
    get,
    path = "/api/groups/{id}/pings",
    params(
        ("id" = Uuid, Path, description = "Group ID"),
        ("state" = Option<PingState>, Query, description = "Filter by ping state")
    ),
    responses(
        (status = 200, description = "List of pings", body = Vec<Ping>),
        (status = 404, description = "Group not found", body = crate::models::ApiError)
    ),
    tag = "Groups"
)]
pub async fn list_group_pings(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Query(query): Query<ListPingsQuery>,
) -> Result<Json<Vec<Ping>>, AppError> {
    if !state.groups.exists(&id) {
        return Err(AppError::NotFound("Group".to_string()));
    }

    let mut pings = state.get_group_pings(id);

    if let Some(ping_state) = query.state {
        pings.retain(|p| p.state == ping_state);
    }

    Ok(Json(pings))
}
