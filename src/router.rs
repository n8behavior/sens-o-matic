//! The route table, and the OpenAPI document that describes it.
//!
//! [`create_router`] registers every handler of [`crate::handlers`] and
//! serves, besides the API itself:
//!
//! | Path | What |
//! |---|---|
//! | `/health` | `ok`, to tell that the server is up |
//! | `/api-docs/openapi.json` | the OpenAPI document |
//! | `/swagger-ui` | the same document, rendered for people |
//!
//! # The OpenAPI document
//!
//! The document is not written by hand. It is built from the code:
//!
//! - Each handler carries a `#[utoipa::path]` attribute with its method, path,
//!   parameters and status codes, and the router takes the route from it. A
//!   handler without the attribute does not compile into [`routes!`].
//! - Each type of [`crate::models`] that appears in a request or a response
//!   derives [`utoipa::ToSchema`] and is listed in `ApiDoc`.
//! - The *doc comments* of the handlers and of the schema types, and of their
//!   fields and variants, become the descriptions in the document. That is why
//!   they are written for the reader of the API, and why they carry no
//!   intra-doc links: a link would show up in the JSON as plain text.
//!
//! A new endpoint is thus described once, in its doc comment and its
//! attribute, and the document follows.

use axum::Router;
use utoipa::OpenApi;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use utoipa_swagger_ui::SwaggerUi;

use crate::handlers::{groups, pings, responses, users};
use crate::models;
use crate::state::AppState;

/// The skeleton of the OpenAPI document: title, tags and the schema types.
///
/// The paths are added by [`create_router`] from the handlers.
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
        (name = "Responses", description = "Ping responses")
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
        models::PingLifecycle,
        models::CreatePingRequest,
        models::CancelPingRequest,
        models::TriggerMatchRequest,
        models::Response,
        models::Availability,
        models::ResponsePreferences,
        models::CreateResponseRequest,
        models::UpdateResponseRequest,
        models::HangoutData,
        models::AttendeeStatus,
        models::Timeline,
        models::ConfirmHangoutRequest,
        models::UpdateAttendeeStatusRequest,
        models::MatchResults,
        models::TimeOverlap,
    ))
)]
struct ApiDoc;

/// Builds the router of the API, with its OpenAPI document and Swagger UI.
///
/// The router holds `state` and answers every endpoint of [`crate::handlers`]. It
/// adds no layer: tracing and CORS are the business of the caller (see
/// `src/main.rs`).
///
/// # Examples
///
/// ```
/// use sens_o_matic::{AppState, create_router};
///
/// let app = create_router(AppState::new());
/// // Serve it with `axum::serve`, as `src/main.rs` does.
/// # let _ = app;
/// ```
pub fn create_router(state: AppState) -> Router {
    let (router, api) = OpenApiRouter::with_openapi(ApiDoc::openapi())
        // Users
        .routes(routes!(users::create_user))
        .routes(routes!(
            users::get_user,
            users::update_user,
            users::delete_user
        ))
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
        .routes(routes!(pings::activate_ping))
        .routes(routes!(pings::complete_ping))
        .routes(routes!(pings::update_attendee_status))
        // Responses
        .routes(routes!(responses::create_response))
        .routes(routes!(responses::update_response))
        .with_state(state)
        .split_for_parts();

    router
        .merge(SwaggerUi::new("/swagger-ui").url("/api-docs/openapi.json", api))
        .route("/health", axum::routing::get(health))
}

/// Answers `ok`, so that a load balancer or a script can tell the server is up.
async fn health() -> &'static str {
    "ok"
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};
    use utoipa::{OpenApi, PartialSchema};

    use super::ApiDoc;
    use crate::models::UserId;

    /// The fields that hold an id say `value_type = String, format = "uuid"`
    /// (see [`crate::models::id`]), and `UserId::schema()` says it for the
    /// parameters. The document must spell an id the same way everywhere.
    #[test]
    fn ids_are_uuid_strings() {
        let id = serde_json::to_value(UserId::schema()).unwrap();
        assert_eq!(id, json!({ "type": "string", "format": "uuid" }));

        let doc = serde_json::to_value(ApiDoc::openapi()).unwrap();
        let schemas = &doc["components"]["schemas"];
        let type_and_format = |pointer: &str| -> Value {
            let schema = schemas
                .pointer(pointer)
                .unwrap_or_else(|| panic!("the document has no schema at {pointer}"));
            json!({ "type": schema["type"], "format": schema["format"] })
        };
        for pointer in [
            "/User/properties/id",
            "/Group/properties/id",
            "/Group/properties/members/items",
            "/CreateGroupRequest/properties/creator_id",
            "/CreatePingRequest/properties/initiator",
            "/CreatePingRequest/properties/group",
            "/Response/properties/id",
            "/Response/properties/user",
            "/HangoutData/properties/confirmed_attendees/items",
            "/HangoutData/properties/attendee_statuses/propertyNames",
            "/MatchResults/properties/ping_id",
        ] {
            assert_eq!(type_and_format(pointer), id, "at {pointer}");
        }
    }
}
