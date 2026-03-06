use anyhow::{Context, Result};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePool, SqlitePoolOptions};
use std::path::Path;
use std::str::FromStr;

/// Load sqlite-vec at process level via sqlite3_auto_extension.
/// Must be called once before creating any connections.
pub fn ensure_sqlite_vec_loaded() {
    use once_cell::sync::OnceCell;
    static LOADED: OnceCell<()> = OnceCell::new();
    LOADED.get_or_init(|| unsafe {
        libsqlite3_sys::sqlite3_auto_extension(Some(std::mem::transmute(
            sqlite_vec::sqlite3_vec_init as *const (),
        )));
    });
}

/// Open (or create) the SQLite pool. Enables WAL mode, foreign keys, and sqlite-vec.
pub async fn open_pool(db_path: &Path) -> Result<SqlitePool> {
    ensure_sqlite_vec_loaded();

    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("Cannot create directory {}", parent.display()))?;
    }

    let url = format!("sqlite://{}?mode=rwc", db_path.display());
    let opts = SqliteConnectOptions::from_str(&url)?
        .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
        .foreign_keys(true)
        .create_if_missing(true);

    let pool = SqlitePoolOptions::new()
        .max_connections(1) // SQLite: single writer
        .connect_with(opts)
        .await
        .with_context(|| format!("Cannot open database at {}", db_path.display()))?;

    Ok(pool)
}
