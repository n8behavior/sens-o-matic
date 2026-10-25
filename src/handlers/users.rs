//! The endpoints for users: create, read, change and delete one, and list the groups
//! of one.
//!
//! There is no authentication. The id of a user in a path or in a body is taken as
//! the word of the client.

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
};
use validator::Validate;

use crate::models::{AppError, AppJson, CreateUserRequest, Group, UpdateUserRequest, User, UserId};
use crate::state::AppState;

/// Creates a user.
///
/// The server assigns the `id`. The name must be 1 to 100 characters.
#[utoipa::path(
    post,
    path = "/api/users",
    request_body = CreateUserRequest,
    responses(
        (status = 201, description = "User created successfully", body = User),
        (status = 400, description = "Invalid request data", body = crate::models::ApiError)
    ),
    tag = "Users"
)]
pub async fn create_user(
    State(state): State<AppState>,
    AppJson(request): AppJson<CreateUserRequest>,
) -> Result<impl IntoResponse, AppError> {
    request.validate()?;

    let user = User::new(request);
    state.users.insert(user.id, user.clone());

    Ok((StatusCode::CREATED, Json(user)))
}

/// Returns a user.
#[utoipa::path(
    get,
    path = "/api/users/{id}",
    params(
        ("id" = inline(UserId), Path, description = "User ID")
    ),
    responses(
        (status = 200, description = "User found", body = User),
        (status = 404, description = "User not found", body = crate::models::ApiError)
    ),
    tag = "Users"
)]
pub async fn get_user(
    State(state): State<AppState>,
    Path(id): Path<UserId>,
) -> Result<Json<User>, AppError> {
    state
        .users
        .get(&id)
        .map(Json)
        .ok_or_else(|| AppError::NotFound("User".to_string()))
}

/// Changes a user.
///
/// Only the fields that are sent change. The avatar and the preferences can be
/// replaced but not cleared, and the preferences are replaced as a whole, not
/// merged field by field.
#[utoipa::path(
    patch,
    path = "/api/users/{id}",
    params(
        ("id" = inline(UserId), Path, description = "User ID")
    ),
    request_body = UpdateUserRequest,
    responses(
        (status = 200, description = "User updated successfully", body = User),
        (status = 400, description = "Invalid request data", body = crate::models::ApiError),
        (status = 404, description = "User not found", body = crate::models::ApiError)
    ),
    tag = "Users"
)]
pub async fn update_user(
    State(state): State<AppState>,
    Path(id): Path<UserId>,
    AppJson(request): AppJson<UpdateUserRequest>,
) -> Result<Json<User>, AppError> {
    request.validate()?;

    state
        .users
        .update(&id, |user| user.update(request.clone()))
        .map(Json)
        .ok_or_else(|| AppError::NotFound("User".to_string()))
}

/// Deletes a user.
///
/// Nothing else changes: the user stays in the `members` of its groups and keeps its
/// answers to pings.
#[utoipa::path(
    delete,
    path = "/api/users/{id}",
    params(
        ("id" = inline(UserId), Path, description = "User ID")
    ),
    responses(
        (status = 204, description = "User deleted successfully"),
        (status = 404, description = "User not found", body = crate::models::ApiError)
    ),
    tag = "Users"
)]
pub async fn delete_user(
    State(state): State<AppState>,
    Path(id): Path<UserId>,
) -> Result<StatusCode, AppError> {
    state
        .users
        .remove(&id)
        .map(|_| StatusCode::NO_CONTENT)
        .ok_or_else(|| AppError::NotFound("User".to_string()))
}

/// Lists the groups that a user is a member of, in no particular order.
#[utoipa::path(
    get,
    path = "/api/users/{id}/groups",
    params(
        ("id" = inline(UserId), Path, description = "User ID")
    ),
    responses(
        (status = 200, description = "List of groups the user belongs to", body = Vec<Group>),
        (status = 404, description = "User not found", body = crate::models::ApiError)
    ),
    tag = "Users"
)]
pub async fn list_user_groups(
    State(state): State<AppState>,
    Path(id): Path<UserId>,
) -> Result<Json<Vec<Group>>, AppError> {
    if !state.users.exists(&id) {
        return Err(AppError::NotFound("User".to_string()));
    }

    let groups = state.get_user_groups(id);
    Ok(Json(groups))
}
