//! Two requests that arrive together cannot both pass the same check.
//!
//! A handler used to check the state of a copy of the ping and then write to the
//! stored one, so two requests could both pass the check. The check and the
//! transition now run in one call under the lock of the store (see
//! [`sens_o_matic::models::ping`]). Each test starts two threads at the same
//! moment, on the handlers of the API, and repeats that for many pings.

use std::{future::Future, sync::Barrier, thread};

use axum::{
    Json,
    extract::{Path, State},
};
use sens_o_matic::{
    AppState,
    handlers::{pings::trigger_match, responses::create_response},
    models::{
        AnyPing, AppError, AppJson, Availability, CreateGroupRequest, CreatePingRequest,
        CreateResponseRequest, Group, Ping, PingId, PingSent, PingState, Response,
        TriggerMatchRequest, UserId,
    },
};

/// How many pings each test races over. A race is not found every time, so
/// the test tries many.
const ROUNDS: usize = 200;

/// Runs a handler to its end on the thread that calls it.
fn block_on<F: Future>(future: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap()
        .block_on(future)
}

/// A state with a group of one member, and a ping that the member sent to it.
/// The ping is not in the store yet.
fn state_with_ping() -> (AppState, UserId, Ping<PingSent>) {
    let state = AppState::new();
    let member = UserId::random();
    let group = Group::new(CreateGroupRequest {
        name: "Friday crew".parse().unwrap(),
        creator_id: member,
    });
    let ping = Ping::new(CreatePingRequest {
        initiator: member,
        group: group.id,
        activity_type: "drinks".parse().unwrap(),
        rough_timing: "tonight".parse().unwrap(),
        vibe: None,
    });
    state.groups.insert(group.id, group);
    (state, member, ping)
}

/// Runs `request` on two threads that start together, and returns the two
/// results.
fn race<T: Send>(request: impl Fn() -> T + Sync) -> [T; 2] {
    let start = Barrier::new(2);
    thread::scope(|scope| {
        let threads: [_; 2] = std::array::from_fn(|_| {
            scope.spawn(|| {
                start.wait();
                request()
            })
        });
        threads.map(|thread| thread.join().unwrap())
    })
}

/// Tells the winner from the loser: exactly one succeeds, and the other gets a
/// conflict.
fn assert_one_wins_one_conflicts<T: std::fmt::Debug>(results: [Result<T, AppError>; 2]) {
    let (won, lost): (Vec<_>, Vec<_>) = results.into_iter().partition(Result::is_ok);
    assert_eq!(won.len(), 1, "exactly one succeeds: {won:?} and {lost:?}");
    assert!(
        matches!(lost.as_slice(), [Err(AppError::Conflict(_))]),
        "the other gets a conflict: {lost:?}"
    );
}

#[test]
fn two_threads_trigger_the_matching_of_one_ping_and_exactly_one_succeeds() {
    for _ in 0..ROUNDS {
        let (state, initiator, ping) = state_with_ping();
        let ping_id = ping.id;
        let ping = ping.respond(Response::new(CreateResponseRequest {
            user: initiator,
            answer: true,
            availability: Some(
                Availability::new(
                    "2024-12-15T18:00:00Z".parse().unwrap(),
                    "2024-12-15T21:00:00Z".parse().unwrap(),
                )
                .unwrap(),
            ),
            preferences: None,
        }));
        state.pings.insert(ping_id, AnyPing::from(ping));

        let results = race(|| {
            block_on(trigger_match(
                State(state.clone()),
                Path(ping_id),
                AppJson(TriggerMatchRequest { user_id: initiator }),
            ))
            .map(|Json(ping)| ping.state())
        });

        assert_one_wins_one_conflicts(results);
        let ping = state.pings.get(&ping_id).unwrap();
        assert_eq!(ping.state(), PingState::Matching);
    }
}

#[test]
fn two_threads_answer_one_ping_as_one_member_and_exactly_one_succeeds() {
    for _ in 0..ROUNDS {
        let (state, member, ping) = state_with_ping();
        let ping_id: PingId = ping.id;
        state.pings.insert(ping_id, AnyPing::from(ping));

        let results = race(|| {
            block_on(create_response(
                State(state.clone()),
                Path(ping_id),
                AppJson(CreateResponseRequest {
                    user: member,
                    answer: false,
                    availability: None,
                    preferences: None,
                }),
            ))
            .map(|_| ())
        });

        assert_one_wins_one_conflicts(results);
        let ping = state.pings.get(&ping_id).unwrap();
        assert_eq!(ping.responses().len(), 1, "the member answered once");
    }
}
