//! Entry point of the server.
//!
//! Serves `sens_o_matic::create_router` on `0.0.0.0:3000`; the port is not
//! configurable. Request tracing and a permissive CORS policy are layered on
//! here and are not part of the router. The log filter comes from `RUST_LOG`
//! and defaults to `sens_o_matic=debug,tower_http=debug`.

use std::net::SocketAddr;

use tokio::net::TcpListener;
use tower_http::{cors::CorsLayer, trace::TraceLayer};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use sens_o_matic::{AppState, create_router};

// ANCHOR: main
#[tokio::main]
async fn main() {
    // Initialize tracing
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "sens_o_matic=debug,tower_http=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    // Create application state
    let state = AppState::new();

    // Create router
    let app = create_router(state)
        .layer(TraceLayer::new_for_http())
        .layer(CorsLayer::permissive());

    // Start server
    let addr = SocketAddr::from(([0, 0, 0, 0], 3000));
    tracing::info!("Listening on {}", addr);

    let listener = TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
// ANCHOR_END: main
