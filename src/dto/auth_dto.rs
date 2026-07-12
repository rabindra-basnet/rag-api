use std::sync::LazyLock;

use regex::Regex;
use serde::Deserialize;
use validator::Validate;

static USERNAME_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[a-zA-Z0-9_]+$").unwrap());

#[derive(Deserialize, Validate)]
pub struct RegisterReq {
    #[validate(email(message = "must be a valid email address"))]
    #[validate(length(max = 254, message = "email too long"))]
    pub email: String,
    #[validate(length(min = 8, max = 128, message = "password must be 8-128 characters"))]
    pub password: String,
    #[validate(length(min = 1, max = 100, message = "name must be 1-100 characters"))]
    #[serde(default)]
    pub name: Option<String>,
    #[validate(length(min = 3, max = 50, message = "username must be 3-50 characters"))]
    #[validate(regex(path = "crate::dto::auth_dto::USERNAME_RE", message = "username must be alphanumeric with underscores"))]
    #[serde(default)]
    pub username: Option<String>,
}

#[derive(Deserialize, Validate)]
pub struct LoginReq {
    #[validate(length(min = 1, max = 254, message = "email required"))]
    pub email: String,
    #[validate(length(min = 1, max = 128, message = "password required"))]
    pub password: String,
}

#[derive(Deserialize, Validate)]
pub struct UpdateProfileReq {
    #[validate(length(min = 1, max = 100, message = "name must be 1-100 characters"))]
    #[serde(default)]
    pub name: Option<String>,
    #[validate(length(min = 3, max = 50, message = "username must be 3-50 characters"))]
    #[validate(regex(path = "crate::dto::auth_dto::USERNAME_RE", message = "username must be alphanumeric with underscores"))]
    #[serde(default)]
    pub username: Option<String>,
}
