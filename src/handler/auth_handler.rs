use axum::extract::State;
use axum::Json;
use axum_extra::extract::cookie::{Cookie, CookieJar, SameSite};
use serde_json::{json, Value};
use time::Duration as CookieDuration;
use chrono::Utc;

use crate::dto::auth_dto::{LoginReq, RegisterReq};
use crate::dto::user_dto::UserReadDto;
use crate::error::auth_error::AuthError;
use crate::error::request_error::ValidatedJson;
use crate::error::ApiError;
use crate::state::app_state::AppState;

use crate::service::auth_service;
use crate::service::user_service;

pub const ACCESS_COOKIE: &str = "access_token";
pub const REFRESH_COOKIE: &str = "refresh_token";

fn valid_email(email: &str) -> bool {
    let email = email.trim();
    email.len() <= 254 && email.contains('@') && !email.starts_with('@') && !email.ends_with('@')
}

fn access_cookie(state: &AppState, token: &str) -> Cookie<'static> {
    Cookie::build((ACCESS_COOKIE, token.to_string()))
        .http_only(true)
        .secure(state.cfg.cookie_secure)
        .same_site(SameSite::Lax)
        .path("/")
        .max_age(CookieDuration::minutes(state.cfg.access_ttl_minutes))
        .build()
}

fn refresh_cookie(state: &AppState, token: &str) -> Cookie<'static> {
    Cookie::build((REFRESH_COOKIE, token.to_string()))
        .http_only(true)
        .secure(state.cfg.cookie_secure)
        .same_site(SameSite::Lax)
        .path("/")
        .max_age(CookieDuration::days(state.cfg.refresh_ttl_days))
        .build()
}

fn clear_access_cookie(state: &AppState) -> Cookie<'static> {
    let mut c = access_cookie(state, "");
    c.set_max_age(CookieDuration::ZERO);
    c
}

fn clear_refresh_cookie(state: &AppState) -> Cookie<'static> {
    let mut c = refresh_cookie(state, "");
    c.set_max_age(CookieDuration::ZERO);
    c
}

pub async fn register(
    State(state): State<AppState>,
    jar: CookieJar,
    ValidatedJson(req): ValidatedJson<RegisterReq>,
) -> Result<(CookieJar, Json<Value>), ApiError> {
    let email = req.email.trim().to_lowercase();
    if !valid_email(&email) {
        return Err(ApiError::Auth(AuthError::BadRequest("invalid email".into())));
    }
    auth_service::validate_password_strength(&req.password)?;

    if user_service::find_by_email(&state.db, &email).await?.is_some() {
        return Err(ApiError::Auth(AuthError::Conflict("email already registered".into())));
    }

    let name = req.name.as_deref().map(|n| n.trim()).filter(|n| !n.is_empty());
    if let Some(ref username) = req.username {
        if user_service::find_by_username(&state.db, username).await?.is_some() {
            return Err(ApiError::Auth(AuthError::Conflict("username already taken".into())));
        }
    }

    let user = user_service::create_user(
        &state.db,
        &email,
        &req.password,
        name,
        req.username.as_deref(),
    )
    .await?;

    let profile = UserReadDto::from(&user);

    if state.cfg.signup_login {
        let access_token = auth_service::issue_access_token(
            &state.cfg.jwt_secret,
            &user.id,
            &user.email,
            state.cfg.access_ttl_minutes,
        )?;
        let refresh_token = auth_service::issue_refresh_token(
            &state.cfg.refresh_jwt_secret,
            &user.id,
            &user.id,
            state.cfg.refresh_ttl_days,
        )?;

        let jar = jar
            .add(access_cookie(&state, &access_token))
            .add(refresh_cookie(&state, &refresh_token));
        return Ok((
            jar,
            Json(json!({
                "access_token": access_token,
                "user": profile,
            })),
        ));
    }

    Ok((
        jar,
        Json(json!({
            "user": profile,
        })),
    ))
}

pub async fn login(
    State(state): State<AppState>,
    jar: CookieJar,
    ValidatedJson(req): ValidatedJson<LoginReq>,
) -> Result<(CookieJar, Json<Value>), ApiError> {
    let email = req.email.trim().to_lowercase();
    let user = user_service::find_by_email(&state.db, &email).await?
        .ok_or(ApiError::Auth(AuthError::InvalidCredentials))?;

    let hash = crate::repository::user_repository::get_account_password(&state.db, &user.id).await?
        .ok_or(ApiError::Auth(AuthError::InvalidCredentials))?;

    if !auth_service::verify_password(&req.password, &hash) {
        return Err(ApiError::Auth(AuthError::InvalidCredentials));
    }

    let access_token = auth_service::issue_access_token(
        &state.cfg.jwt_secret,
        &user.id,
        &user.email,
        state.cfg.access_ttl_minutes,
    )?;
    let refresh_token = auth_service::issue_refresh_token(
        &state.cfg.refresh_jwt_secret,
        &user.id,
        &user.id,
        state.cfg.refresh_ttl_days,
    )?;

    let profile = UserReadDto::from(&user);
    let jar = jar
        .add(access_cookie(&state, &access_token))
        .add(refresh_cookie(&state, &refresh_token));
    Ok((
        jar,
        Json(json!({
            "access_token": access_token,
            "user": profile,
        })),
    ))
}

pub async fn refresh(
    State(state): State<AppState>,
    jar: CookieJar,
) -> Result<(CookieJar, Json<Value>), ApiError> {
    let token = jar
        .get(REFRESH_COOKIE)
        .map(|c| c.value().to_string())
        .ok_or(ApiError::Auth(AuthError::Unauthorized))?;

    let claims = auth_service::verify_refresh_token(&state.cfg.refresh_jwt_secret, &token)?;
    if claims.typ != "refresh" {
        return Err(ApiError::Auth(AuthError::Unauthorized));
    }

    let user = user_service::find_by_id(&state.db, &claims.sub)
        .await?
        .ok_or(ApiError::Auth(AuthError::Unauthorized))?;

    let access_token = auth_service::issue_access_token(
        &state.cfg.jwt_secret,
        &user.id,
        &user.email,
        state.cfg.access_ttl_minutes,
    )?;
    let refresh_token = auth_service::issue_refresh_token(
        &state.cfg.refresh_jwt_secret,
        &claims.sub,
        &claims.sid,
        state.cfg.refresh_ttl_days,
    )?;

    let jar = jar
        .add(access_cookie(&state, &access_token))
        .add(refresh_cookie(&state, &refresh_token));
    Ok((
        jar,
        Json(json!({
            "access_token": access_token,
        })),
    ))
}

pub async fn logout(
    State(state): State<AppState>,
    jar: CookieJar,
    headers: axum::http::HeaderMap,
) -> Result<(CookieJar, Json<Value>), ApiError> {
    if let Some(header) = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
    {
        if let Ok(claims) = auth_service::verify_access_token(&state.cfg.jwt_secret, header) {
            let exp = chrono::DateTime::from_timestamp(claims.exp, 0)
                .map(|t| t.to_rfc3339())
                .unwrap_or_else(|| Utc::now().to_rfc3339());
            auth_service::revoke_token(&state.db, &claims.jti, &exp).await?;
        }
    }

    if let Some(refresh) = jar.get(REFRESH_COOKIE).map(|c| c.value().to_string()) {
        if let Ok(claims) = auth_service::verify_refresh_token(&state.cfg.refresh_jwt_secret, &refresh) {
            let exp = chrono::DateTime::from_timestamp(claims.exp, 0)
                .map(|t| t.to_rfc3339())
                .unwrap_or_else(|| Utc::now().to_rfc3339());
            auth_service::revoke_token(&state.db, &claims.jti, &exp).await?;
        }
    }

    let jar = jar
        .remove(clear_access_cookie(&state))
        .remove(clear_refresh_cookie(&state));
    Ok((jar, Json(json!({ "ok": true }))))
}
