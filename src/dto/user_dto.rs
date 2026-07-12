use serde::Serialize;

use crate::entity::user::User;

#[derive(Clone, Debug, Serialize)]
pub struct UserReadDto {
    pub id: String,
    pub name: Option<String>,
    pub email: Option<String>,
    pub image: Option<String>,
    pub username: Option<String>,
    pub display_username: Option<String>,
    pub created_at: String,
}

impl From<&User> for UserReadDto {
    fn from(u: &User) -> Self {
        Self {
            id: u.id.clone(),
            name: u.name.clone(),
            email: u.email.clone(),
            image: u.image.clone(),
            username: u.username.clone(),
            display_username: u.display_username.clone(),
            created_at: u.created_at.clone(),
        }
    }
}
