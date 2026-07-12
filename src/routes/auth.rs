use axum::routing::post;
use axum::Router;

use crate::handler::auth_handler;
use crate::state::app_state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/auth/register", post(auth_handler::register))
        .route("/auth/login", post(auth_handler::login))
        .route("/auth/refresh", post(auth_handler::refresh))
        .route("/auth/logout", post(auth_handler::logout))
}
