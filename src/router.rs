use axum::Router;
use utoipa::OpenApi;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use utoipa_swagger_ui::SwaggerUi;

use crate::handlers::{groups, hangouts, pings, responses, users};
use crate::models;
use crate::state::AppState;

#[derive(OpenApi)]
#[openapi(
    info(
        title = "Sens-O-Matic API",
        description = "API for coordinating spontaneous meetups with friends",
        version = "0.1.0"
    ),
    tags(
        (name = "Users", description = "User management"),
        (name = "Groups", description = "Group management"),
        (name = "Pings", description = "Ping lifecycle"),
        (name = "Responses", description = "Ping responses"),
        (name = "Hangouts", description = "Hangout management")
    ),
    components(schemas(
        models::ApiError,
        models::User,
        models::UserPreferences,
        models::Location,
        models::CreateUserRequest,
        models::UpdateUserRequest,
        models::Group,
        models::CreateGroupRequest,
        models::JoinGroupRequest,
        models::LeaveGroupRequest,
        models::RegenerateInviteRequest,
        models::Ping,
        models::PingState,
        models::CreatePingRequest,
        models::CancelPingRequest,
        models::TriggerMatchRequest,
        models::Response,
        models::Availability,
        models::ResponsePreferences,
        models::CreateResponseRequest,
        models::UpdateResponseRequest,
        models::Hangout,
        models::HangoutStatus,
        models::AttendeeStatus,
        models::Timeline,
        models::ConfirmHangoutRequest,
        models::UpdateAttendeeStatusRequest,
        models::MatchResults,
        models::TimeOverlap,
    ))
)]
struct ApiDoc;

pub fn create_router(state: AppState) -> Router {
    let (router, api) = OpenApiRouter::with_openapi(ApiDoc::openapi())
        // Users
        .routes(routes!(users::create_user))
        .routes(routes!(users::get_user, users::update_user, users::delete_user))
        .routes(routes!(users::list_user_groups))
        // Groups
        .routes(routes!(groups::create_group))
        .routes(routes!(groups::get_group))
        .routes(routes!(groups::join_group))
        .routes(routes!(groups::leave_group))
        .routes(routes!(groups::regenerate_invite_code))
        .routes(routes!(groups::list_group_pings))
        // Pings
        .routes(routes!(pings::create_ping))
        .routes(routes!(pings::get_ping))
        .routes(routes!(pings::cancel_ping))
        .routes(routes!(pings::trigger_match))
        .routes(routes!(pings::get_match_results))
        .routes(routes!(pings::confirm_hangout))
        // Responses
        .routes(routes!(responses::create_response))
        .routes(routes!(responses::update_response))
        // Hangouts
        .routes(routes!(hangouts::get_hangout))
        .routes(routes!(hangouts::activate_hangout))
        .routes(routes!(hangouts::complete_hangout))
        .routes(routes!(hangouts::update_attendee_status))
        .with_state(state)
        .split_for_parts();

    router.merge(SwaggerUi::new("/swagger-ui").url("/api-docs/openapi.json", api))
}
