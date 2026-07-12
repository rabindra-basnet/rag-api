use sqlx::migrate::Migrator;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;
use std::path::Path;
use std::str::FromStr;

/// The one place that names the concrete database. SQLite for development
/// and testing today; switching to Postgres means changing these aliases
/// (and init()) — the rest of the codebase only uses `Db` / `DbTx`.
pub type Db = SqlitePool;
pub type DbTx<'t> = sqlx::Transaction<'t, sqlx::Sqlite>;

/// Resolved at runtime relative to the working directory — run the binary
/// from the project/deploy root, with the migrations/ folder next to it.
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

    // Versioned migrations loaded at runtime from the given directory
    // (deploy the migrations/ folder alongside the binary).
    Migrator::new(Path::new(MIGRATIONS_PATH)).await?.run(&pool).await?;

    Ok(pool)
}
