//! The update of a response refuses what the create of a response refuses.
//!
//! The rules live in the types of [`sens_o_matic::models::value`], which both
//! requests use, so a body that breaks one is refused with `400` when it is
//! read, whichever endpoint it goes to. The Hurl suite (`just test-api`) covers
//! the create; this test sends the same bodies to the update, through the
//! router.

use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Method, Request, StatusCode},
};
use sens_o_matic::{AppState, create_router};
use serde_json::{Value, json};
use tower::ServiceExt;

/// Sends one JSON request through the router and returns the status and the
/// JSON body of the answer.
async fn send(app: &Router, method: Method, uri: &str, body: Value) -> (StatusCode, Value) {
    let request = Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap();
    let response = app.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

/// A refusal is a `400` with the usual `ApiError` body: the `error` code, and
/// a `message` that says what is wrong and which values are allowed.
fn assert_refused(what: &str, answer: (StatusCode, Value), message_has: &[&str]) {
    let (status, body) = answer;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{what}: body {body}");
    assert_eq!(body["error"], "bad_request", "{what}");
    let message = body["message"].as_str().expect("the body has a message");
    for part in message_has {
        assert!(message.contains(part), "{what}: {message:?} lacks {part:?}");
    }
}

#[tokio::test]
async fn update_response_refuses_what_create_response_refuses() {
    let app = create_router(AppState::new());

    // A ping with one answer that is valid.
    let (_, user) = send(&app, Method::POST, "/api/users", json!({ "name": "Ada" })).await;
    let user = user["id"].as_str().unwrap().to_string();
    let (_, group) = send(
        &app,
        Method::POST,
        "/api/groups",
        json!({ "name": "Friday crew", "creator_id": user }),
    )
    .await;
    let (_, ping) = send(
        &app,
        Method::POST,
        "/api/pings",
        json!({
            "initiator": user,
            "group": group["id"],
            "activity_type": "drinks",
            "rough_timing": "tonight",
        }),
    )
    .await;
    let responses = format!("/api/pings/{}/responses", ping["id"].as_str().unwrap());
    let (status, response) = send(
        &app,
        Method::POST,
        &responses,
        json!({
            "user": user,
            "answer": true,
            "availability": { "earliest": "2024-12-15T18:00:00Z", "latest": "2024-12-15T22:00:00Z" },
            "preferences": { "max_distance": 5.0 },
        }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let one = format!("{responses}/{}", response["id"].as_str().unwrap());

    // The bodies that break a rule, with the words their refusal must have.
    let refused = [
        (
            "a negative max_distance",
            json!({ "preferences": { "max_distance": -5.0 } }),
            ["distance", "0 or more"],
        ),
        (
            "a latest before the earliest",
            json!({ "availability": {
                "earliest": "2024-12-15T22:00:00Z", "latest": "2024-12-15T18:00:00Z" } }),
            ["time window", "end after"],
        ),
        (
            "a latest that is the earliest",
            json!({ "availability": {
                "earliest": "2024-12-15T18:00:00Z", "latest": "2024-12-15T18:00:00Z" } }),
            ["time window", "end after"],
        ),
    ];
    for (what, change, message_has) in refused {
        let mut update = json!({ "user": user });
        update
            .as_object_mut()
            .unwrap()
            .extend(change.as_object().unwrap().clone());

        // The create refuses the body (with an answer by another user, so that
        // only the body can be the reason) ...
        let mut create = update.clone();
        create["user"] = json!(sens_o_matic::models::UserId::random());
        create["answer"] = json!(true);
        let created = send(&app, Method::POST, &responses, create).await;
        assert_refused(what, created, &message_has);

        // ... and so does the update.
        let updated = send(&app, Method::PUT, &one, update).await;
        assert_refused(what, updated, &message_has);
    }

    // The refused updates changed nothing, and a valid update still works.
    let (_, ping) = send(
        &app,
        Method::GET,
        &format!("/api/pings/{}", ping["id"].as_str().unwrap()),
        Value::Null,
    )
    .await;
    assert_eq!(ping["responses"][0]["preferences"]["max_distance"], 5.0);
    assert_eq!(
        ping["responses"][0]["availability"]["latest"],
        "2024-12-15T22:00:00Z"
    );

    let (status, updated) = send(
        &app,
        Method::PUT,
        &one,
        json!({
            "user": user,
            "availability": { "earliest": "2024-12-15T19:00:00Z", "latest": "2024-12-15T21:00:00Z" },
            "preferences": { "max_distance": 0.0 },
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body: {updated}");
    assert_eq!(updated["preferences"]["max_distance"], 0.0);
}
