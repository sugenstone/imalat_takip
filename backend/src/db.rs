use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;
use std::str::FromStr;
use std::time::Duration;

pub async fn init_pool(db_url: &str) -> SqlitePool {
    // sqlite://data/app.db -> klasoru olustur
    let path_part = db_url
        .strip_prefix("sqlite://")
        .or_else(|| db_url.strip_prefix("sqlite:"));
    if let Some(path) = path_part {
        let file_path = path.split('?').next().unwrap_or(path);
        if !file_path.is_empty() && file_path != ":memory:" {
            if let Some(parent) = std::path::Path::new(file_path).parent() {
                let _ = std::fs::create_dir_all(parent);
            }
        }
    }

    let opts = SqliteConnectOptions::from_str(db_url)
        .expect("gecersiz IMTK_DB_URL")
        .create_if_missing(true)
        .foreign_keys(true)
        .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
        .busy_timeout(Duration::from_secs(10));

    let pool = SqlitePoolOptions::new()
        .max_connections(8)
        .connect_with(opts)
        .await
        .expect("sqlite baglantisi kurulamadi");

    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .expect("migration calistirilamadi");

    pool
}
