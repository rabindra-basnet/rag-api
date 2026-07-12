use sqlx::migrate::Migrator;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;
use std::path::Path;
use std::str::FromStr;

pub type Db = SqlitePool;
pub type DbTx<'t> = sqlx::Transaction<'t, sqlx::Sqlite>;

const MIGRATIONS_PATH: &str = "./migrations";

pub async fn init(database_url: &str) -> Result<SqlitePool, sqlx::Error> {
    let opts = SqliteConnectOptions::from_str(database_url)?
        .create_if_missing(true)
        .foreign_keys(true)
        .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal);

    let pool = SqlitePoolOptions::new()
        .max_connections(8)
        .connect_with(opts)
        .await?;

    Migrator::new(Path::new(MIGRATIONS_PATH))
        .await?
        .run(&pool)
        .await?;

    Ok(pool)
}
