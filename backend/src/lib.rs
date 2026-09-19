pub mod attachments;
pub mod audit;
pub mod auth;
pub mod attributes;
pub mod comments;
pub mod config;
pub mod db;
pub mod error;
pub mod invites;
pub mod notifications;
pub mod reports;
pub mod members;
pub mod permissions;
pub mod roles;
pub mod sections;
pub mod state;
pub mod steps;
pub mod tasks;
pub mod teams;
pub mod util;
pub mod work_items;
pub mod work_types;
pub mod workflow_runtime;
pub mod workflows;
pub mod workspaces;
pub mod ws_ctx;

use state::AppState;
use tower_http::trace::TraceLayer;

/// Uygulama router'i: /api altinda REST, kalaninda SvelteKit statik build.
pub fn build_router(state: AppState) -> axum::Router {
    let api = auth::router()
        .merge(workspaces::router())
        .merge(members::router())
        .merge(roles::router())
        .merge(teams::router())
        .merge(sections::router())
        .merge(work_types::router())
        .merge(work_items::router())
        .merge(workflows::router())
        .merge(steps::router())
        .merge(workflow_runtime::router())
        .merge(comments::router())
        .merge(attachments::router())
        .merge(audit::router())
        .merge(notifications::router())
        .merge(reports::router())
        .merge(invites::router())
        .merge(tasks::router());

    let mut app: axum::Router = axum::Router::new()
        .nest("/api", api)
        .layer(TraceLayer::new_for_http())
        .with_state(state.clone());

    // Production: SvelteKit statik build'i ayni sunucudan servis et (tek binary).
    if let Some(dir) = &state.config.static_dir {
        let index = std::path::Path::new(dir).join("index.html");
        let serve_dir = tower_http::services::ServeDir::new(dir)
            .not_found_service(tower_http::services::ServeFile::new(index));
        app = app.fallback_service(serve_dir);
    }
    app
}

/// Test/araclar icin in-memory SQLite ile AppState kurar.
pub async fn test_state() -> AppState {
    use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
    use std::str::FromStr;
    use std::time::Duration;

    let opts = SqliteConnectOptions::from_str("sqlite::memory:")
        .expect("url")
        .create_if_missing(true)
        .foreign_keys(true);
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .min_connections(1)
        .idle_timeout(None)
        .max_lifetime(None)
        .acquire_timeout(Duration::from_secs(5))
        .connect_with(opts)
        .await
        .expect("pool");
    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .expect("migrate");
    AppState {
        db: pool,
        config: config::Config {
            host: "127.0.0.1".into(),
            port: 0,
            db_url: "sqlite::memory:".into(),
            static_dir: None,
            cookie_secure: false,
            smtp_host: String::new(),
            smtp_port: 587,
            smtp_user: String::new(),
            smtp_pass: String::new(),
            smtp_from: String::new(),
            app_url: "http://localhost:5173".into(),
        },
    }
}
