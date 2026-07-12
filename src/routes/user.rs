use axum::routing::get;
use axum::Router;

use crate::handler::user_handler;
use crate::state::app_state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/auth/me", get(user_handler::me).put(user_handler::update_profile))
}
