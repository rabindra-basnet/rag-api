use crate::config::database::Db;
use crate::config::parameter::Config;

#[derive(Clone)]
pub struct AppState {
    pub db: Db,
    pub cfg: Config,
}
