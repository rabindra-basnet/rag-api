use axum::routing::post;
use axum::Router;

use crate::handler::chat_handler;
use crate::state::app_state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new().route("/chat", post(chat_handler::chat))
}
