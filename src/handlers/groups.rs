//! The endpoints for groups: create, read and join one, leave one, replace its invite
//! code, and list its pings.
//!
//! A group is joined with its invite code and left freely. Only the members of a group
//! can send pings to it and answer them; the endpoints of [`super::pings`] and
//! [`super::responses`] check it against the group stored here.

use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
};
use serde::Deserialize;
use validator::Validate;

use crate::models::{
    AppError, AppJson, CreateGroupRequest, Group, GroupId, JoinGroupRequest, LeaveGroupRequest,
    Ping, RegenerateInviteRequest,
};
use crate::state::AppState;

/// Creates a group, with the creator as its only member and a new invite code.
///
/// The server does not check that the creator is a user that exists.
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

/// Returns a group, with its members and its invite code.
#[utoipa::path(
    get,
    path = "/api/groups/{id}",
    params(
        ("id" = inline(GroupId), Path, description = "Group ID")
    ),
    responses(
        (status = 200, description = "Group found", body = Group),
        (status = 404, description = "Group not found", body = crate::models::ApiError)
    ),
    tag = "Groups"
)]
pub async fn get_group(
    State(state): State<AppState>,
    Path(id): Path<GroupId>,
) -> Result<Json<Group>, AppError> {
    state
        .groups
        .get(&id)
        .map(Json)
        .ok_or_else(|| AppError::NotFound("Group".to_string()))
}

/// Adds a user to the group that has the given invite code.
///
/// The code is looked up as it is written, so the case matters. Joining a group that the
/// user is in already is harmless. The server does not check that the user exists.
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

/// Removes a user from a group.
///
/// It succeeds also when the user is not a member. The pings and the answers of the
/// user stay as they are. Nothing stops the last member from leaving.
#[utoipa::path(
    post,
    path = "/api/groups/{id}/leave",
    params(
        ("id" = inline(GroupId), Path, description = "Group ID")
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
    Path(id): Path<GroupId>,
    AppJson(request): AppJson<LeaveGroupRequest>,
) -> Result<StatusCode, AppError> {
    state
        .groups
        .update(&id, |g| g.remove_member(request.user_id))
        .ok_or_else(|| AppError::NotFound("Group".to_string()))?;

    Ok(StatusCode::NO_CONTENT)
}

/// Replaces the invite code of a group with a new one.
///
/// Any member can do it. The old code stops working at once.
#[utoipa::path(
    post,
    path = "/api/groups/{id}/regenerate-invite",
    params(
        ("id" = inline(GroupId), Path, description = "Group ID")
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
    Path(id): Path<GroupId>,
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

/// The query string of the list of the pings of a group.
#[derive(Debug, Deserialize)]
pub struct ListPingsQuery {
    /// Keeps only the pings in this state, by the name that the `state` field of a
    /// ping has (`gathering`, `no_match`, …); see
    /// [`PingLifecycle::state_name`](crate::models::PingLifecycle::state_name).
    pub state: Option<String>,
}

/// Lists the pings that were sent to a group, in no particular order.
///
/// With `state`, only the pings in that state are listed. A value that names no state
/// gives an empty list, not an error.
#[utoipa::path(
    get,
    path = "/api/groups/{id}/pings",
    params(
        ("id" = inline(GroupId), Path, description = "Group ID"),
        ("state" = Option<String>, Query, description = "Filter by ping state")
    ),
    responses(
        (status = 200, description = "List of pings", body = Vec<Ping>),
        (status = 404, description = "Group not found", body = crate::models::ApiError)
    ),
    tag = "Groups"
)]
pub async fn list_group_pings(
    State(state): State<AppState>,
    Path(id): Path<GroupId>,
    Query(query): Query<ListPingsQuery>,
) -> Result<Json<Vec<Ping>>, AppError> {
    if !state.groups.exists(&id) {
        return Err(AppError::NotFound("Group".to_string()));
    }

    let mut pings = state.get_group_pings(id);

    if let Some(filter_state) = query.state {
        pings.retain(|p| p.lifecycle.state_name() == filter_state);
    }

    Ok(Json(pings))
}
