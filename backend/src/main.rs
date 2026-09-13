use imtk_backend::config::Config;
use imtk_backend::{build_router, db, state::AppState};

#[tokio::main]
async fn main() {
    let _ = dotenvy::dotenv();
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "imtk_backend=info,tower_http=info".into()),
        )
        .init();

    let cfg = Config::from_env();
    let pool = db::init_pool(&cfg.db_url).await;
    let state = AppState {
        db: pool.clone(),
        config: cfg.clone(),
    };

    // E-posta kuyrugu isleyicisi (Faz 9)
    imtk_backend::notifications::spawn_email_worker(pool, cfg.clone());

    let app = build_router(state);

    let addr = format!("{}:{}", cfg.host, cfg.port);
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .expect("port dinlenemedi");
    tracing::info!("Sunucu basladi: http://{addr}");
    axum::serve(listener, app).await.expect("sunucu hatasi");
}
