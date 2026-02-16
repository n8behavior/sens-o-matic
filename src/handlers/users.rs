use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
};
use uuid::Uuid;
use validator::Validate;

use crate::models::{AppError, AppJson, CreateUserRequest, Group, UpdateUserRequest, User};
use crate::state::AppState;

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

#[utoipa::path(
    get,
    path = "/api/users/{id}",
    params(
        ("id" = Uuid, Path, description = "User ID")
    ),
    responses(
        (status = 200, description = "User found", body = User),
        (status = 404, description = "User not found", body = crate::models::ApiError)
    ),
    tag = "Users"
)]
pub async fn get_user(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<User>, AppError> {
    state
        .users
        .get(&id)
        .map(Json)
        .ok_or_else(|| AppError::NotFound("User".to_string()))
}

#[utoipa::path(
    patch,
    path = "/api/users/{id}",
    params(
        ("id" = Uuid, Path, description = "User ID")
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
    Path(id): Path<Uuid>,
    AppJson(request): AppJson<UpdateUserRequest>,
) -> Result<Json<User>, AppError> {
    request.validate()?;

    state
        .users
        .update(&id, |user| user.update(request.clone()))
        .map(Json)
        .ok_or_else(|| AppError::NotFound("User".to_string()))
}

#[utoipa::path(
    delete,
    path = "/api/users/{id}",
    params(
        ("id" = Uuid, Path, description = "User ID")
    ),
    responses(
        (status = 204, description = "User deleted successfully"),
        (status = 404, description = "User not found", body = crate::models::ApiError)
    ),
    tag = "Users"
)]
pub async fn delete_user(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    state
        .users
        .remove(&id)
        .map(|_| StatusCode::NO_CONTENT)
        .ok_or_else(|| AppError::NotFound("User".to_string()))
}

#[utoipa::path(
    get,
    path = "/api/users/{id}/groups",
    params(
        ("id" = Uuid, Path, description = "User ID")
    ),
    responses(
        (status = 200, description = "List of groups the user belongs to", body = Vec<Group>),
        (status = 404, description = "User not found", body = crate::models::ApiError)
    ),
    tag = "Users"
)]
pub async fn list_user_groups(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Vec<Group>>, AppError> {
    if !state.users.exists(&id) {
        return Err(AppError::NotFound("User".to_string()));
    }

    let groups = state.get_user_groups(id);
    Ok(Json(groups))
}
