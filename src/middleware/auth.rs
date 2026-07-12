use axum::extract::{FromRequestParts, State};
use axum::http::request::Parts;
use axum::middleware::Next;
use axum::response::Response;
use axum_extra::extract::CookieJar;


use crate::error::auth_error::AuthError;
use crate::error::ApiError;
use crate::handler::auth_handler::ACCESS_COOKIE;
use crate::state::app_state::AppState;
use crate::service::auth_service::verify_access_token;

#[derive(Clone, Debug)]
pub struct AuthUser {
    pub id: String,
    pub email: String,
}

impl<S: Send + Sync> FromRequestParts<S> for AuthUser {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        parts
            .extensions
            .get::<AuthUser>()
            .cloned()
            .ok_or(ApiError::Auth(AuthError::Unauthorized))
    }
}

pub async fn require_auth(
    State(state): State<AppState>,
    mut req: axum::extract::Request,
    next: Next,
) -> Result<Response, ApiError> {
    // 1. Try Authorization header first
    let token = req
        .headers()
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(|s| s.to_string());

    // 2. Fallback to access token cookie
    let token = match token {
        Some(t) => t,
        None => {
            let jar = CookieJar::from_headers(req.headers());
            jar.get(ACCESS_COOKIE)
                .map(|c| c.value().to_string())
                .ok_or(ApiError::Auth(AuthError::Unauthorized))?
        }
    };

    let claims = verify_access_token(&state.cfg.jwt_secret, &token)?;

    if crate::service::auth_service::is_token_revoked(&state.db, &claims.jti).await? {
        return Err(ApiError::Auth(AuthError::Unauthorized));
    }

    req.extensions_mut().insert(AuthUser {
        id: claims.sub,
        email: claims.email.unwrap_or_default(),
    });
    Ok(next.run(req).await)
}
