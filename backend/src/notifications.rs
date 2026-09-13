//! Faz 9: Bildirim sistemi + e-posta kuyrugu (yol haritasi 37-39).
//! Bildirimler event noktalarinda uretilir; mailler request icinde DEGIL,
//! email_outbox kuyrugundan worker ile islenir.

use axum::extract::{Path, State};
use axum::{routing, Json, Router};
use serde::Serialize;

use crate::auth::AuthUser;
use crate::error::{AppError, AppResult};
use crate::state::AppState;
use crate::util;
use crate::ws_ctx::WsCtx;

// ---------------------------------------------------------------------------
// Uretim yardimcilari (runtime event noktalarindan cagrilir)
// ---------------------------------------------------------------------------

/// Kullanicilara bildirim + e-posta kuyrugu yaz.
pub async fn notify_users(
    db: &sqlx::SqlitePool,
    wid: &str,
    user_ids: &[String],
    ntype: &str,
    title: &str,
    message: &str,
    entity_type: &str,
    entity_id: &str,
) {
    let now = util::now();
    for uid in user_ids {
        // e-posta adresini de al (kuyruk icin)
        let email: Option<(String,)> = sqlx::query_as(
            "SELECT email FROM users WHERE id = ?1 AND is_active = 1",
        )
        .bind(uid)
        .fetch_optional(db)
        .await
        .ok()
        .flatten();
        if email.is_none() {
            continue;
        }
        let (email,) = email.unwrap();

        let res = sqlx::query(
            "INSERT INTO notifications (id, workspace_id, user_id, type, title, message, entity_type, entity_id, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        )
        .bind(util::new_id())
        .bind(wid)
        .bind(uid)
        .bind(ntype)
        .bind(title)
        .bind(message)
        .bind(entity_type)
        .bind(entity_id)
        .bind(&now)
        .execute(db)
        .await;
        if let Err(e) = res {
            tracing::warn!("bildirim yazilamadi: {e}");
            continue;
        }

        // E-posta kuyrugu (yol haritasi 39: queue -> worker -> provider)
        let res = sqlx::query(
            "INSERT INTO email_outbox (id, workspace_id, to_email, subject, body, status, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, 'pending', ?6)",
        )
        .bind(util::new_id())
        .bind(wid)
        .bind(&email)
        .bind(title)
        .bind(message)
        .bind(&now)
        .execute(db)
        .await;
        if let Err(e) = res {
            tracing::warn!("outbox yazilamadi: {e}");
        }
    }
}

/// Surecin aktif assignee kullanici id'lerini toplar (user direkt + takim uyeleri).
pub async fn process_assignee_users(
    db: &sqlx::SqlitePool,
    pid: &str,
) -> Vec<String> {
    let rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT assignee_type, assignee_id FROM process_assignments
         WHERE process_instance_id = ?1 AND unassigned_at IS NULL",
    )
    .bind(pid)
    .fetch_all(db)
    .await
    .unwrap_or_default();

    let mut users = Vec::new();
    for (t, id) in rows {
        if t == "user" {
            users.push(id);
        } else {
            let members: Vec<(String,)> = sqlx::query_as(
                "SELECT user_id FROM team_members WHERE team_id = ?1",
            )
            .bind(&id)
            .fetch_all(db)
            .await
            .unwrap_or_default();
            users.extend(members.into_iter().map(|(u,)| u));
        }
    }
    users.dedup();
    users
}

/// Belirli role sahip workspace uyelerinin kullanici id'leri.
/// rol None ise process.approve izni olanlar (izinden turetilemedigi icin
/// rol bazli SQL ile: role_permissions JOIN).
pub async fn users_with_role(db: &sqlx::SqlitePool, wid: &str, role_id: &str) -> Vec<String> {
    let rows: Vec<(String,)> = sqlx::query_as(
        "SELECT m.user_id FROM workspace_members m
         WHERE m.workspace_id = ?1 AND m.role_id = ?2 AND m.status = 'active' AND m.archived_at IS NULL",
    )
    .bind(wid)
    .bind(role_id)
    .fetch_all(db)
    .await
    .unwrap_or_default();
    rows.into_iter().map(|(u,)| u).collect()
}

// ---------------------------------------------------------------------------
// API
// ---------------------------------------------------------------------------

#[derive(Serialize, sqlx::FromRow)]
pub struct NotificationRow {
    pub id: String,
    pub r#type: String,
    pub title: String,
    pub message: String,
    pub entity_type: Option<String>,
    pub entity_id: Option<String>,
    pub read_at: Option<String>,
    pub created_at: String,
}

#[derive(Serialize, sqlx::FromRow)]
pub struct OutboxRow {
    pub id: String,
    pub to_email: String,
    pub subject: String,
    pub status: String,
    pub created_at: String,
    pub sent_at: Option<String>,
}

pub async fn list_mine(
    State(state): State<AppState>,
    user: AuthUser,
    Path(wid): Path<String>,
) -> AppResult<Json<Vec<NotificationRow>>> {
    WsCtx::load(&state.db, &user, &wid).await?;
    let rows = sqlx::query_as::<_, NotificationRow>(
        "SELECT id, type, title, message, entity_type, entity_id, read_at, created_at
         FROM notifications WHERE workspace_id = ?1 AND user_id = ?2
         ORDER BY created_at DESC LIMIT 100",
    )
    .bind(&wid)
    .bind(&user.0.id)
    .fetch_all(&state.db)
    .await?;
    Ok(Json(rows))
}

pub async fn unread_count(
    State(state): State<AppState>,
    user: AuthUser,
    Path(wid): Path<String>,
) -> AppResult<Json<serde_json::Value>> {
    WsCtx::load(&state.db, &user, &wid).await?;
    let c: Option<(i64,)> = sqlx::query_as(
        "SELECT COUNT(*) FROM notifications WHERE workspace_id = ?1 AND user_id = ?2 AND read_at IS NULL",
    )
    .bind(&wid)
    .bind(&user.0.id)
    .fetch_optional(&state.db)
    .await?;
    Ok(Json(serde_json::json!({ "unread": c.map(|(v,)| v).unwrap_or(0) })))
}

pub async fn mark_read(
    State(state): State<AppState>,
    user: AuthUser,
    Path((wid, nid)): Path<(String, String)>,
) -> AppResult<Json<serde_json::Value>> {
    WsCtx::load(&state.db, &user, &wid).await?;
    sqlx::query(
        "UPDATE notifications SET read_at = ?1 WHERE id = ?2 AND user_id = ?3 AND read_at IS NULL",
    )
    .bind(util::now())
    .bind(&nid)
    .bind(&user.0.id)
    .execute(&state.db)
    .await?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

pub async fn mark_all_read(
    State(state): State<AppState>,
    user: AuthUser,
    Path(wid): Path<String>,
) -> AppResult<Json<serde_json::Value>> {
    WsCtx::load(&state.db, &user, &wid).await?;
    sqlx::query(
        "UPDATE notifications SET read_at = ?1 WHERE workspace_id = ?2 AND user_id = ?3 AND read_at IS NULL",
    )
    .bind(util::now())
    .bind(&wid)
    .bind(&user.0.id)
    .execute(&state.db)
    .await?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

/// Outbox durumu — yalniz yoneticiler.
pub async fn outbox(
    State(state): State<AppState>,
    user: AuthUser,
    Path(wid): Path<String>,
) -> AppResult<Json<Vec<OutboxRow>>> {
    let ctx = WsCtx::load(&state.db, &user, &wid).await?;
    if !ctx.is_owner && !ctx.permissions.contains("role.manage") {
        return Err(AppError::Forbidden(
            "E-posta kuyrugunu yoneticiler goruntulebilir".into(),
        ));
    }
    let rows = sqlx::query_as::<_, OutboxRow>(
        "SELECT id, to_email, subject, status, created_at, sent_at
         FROM email_outbox WHERE workspace_id = ?1
         ORDER BY created_at DESC LIMIT 100",
    )
    .bind(&wid)
    .fetch_all(&state.db)
    .await?;
    Ok(Json(rows))
}

// ---------------------------------------------------------------------------
// E-posta worker (yol haritasi 39: Queue -> Worker -> Provider)
// ---------------------------------------------------------------------------

/// Tek mail gonderimi: SMTP yapilandirilmisssa gercek, degilse simulasyon (log).
async fn send_email(cfg: &crate::config::Config, to: &str, subject: &str, body: &str) -> Result<(), String> {
    if !cfg.smtp_configured() {
        tracing::info!("[EMAIL-SIM] to={to} subject={subject} body={body}");
        return Ok(());
    }
    use lettre::message::header::ContentType;
    use lettre::transport::smtp::authentication::Credentials;
    use lettre::{AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor};

    let from: lettre::message::Mailbox = cfg
        .smtp_from
        .parse()
        .map_err(|e| format!("gecersiz IMTK_SMTP_FROM: {e}"))?;
    let to_mailbox: lettre::message::Mailbox = to
        .parse()
        .map_err(|e| format!("gecersiz alici: {e}"))?;

    let email = Message::builder()
        .from(from)
        .to(to_mailbox)
        .subject(subject)
        .header(ContentType::TEXT_PLAIN)
        .body(body.to_string())
        .map_err(|e| format!("mail olusturulamadi: {e}"))?;

    let transporter = AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&cfg.smtp_host)
        .map_err(|e| format!("SMTP relay kurulumu: {e}"))?
        .port(cfg.smtp_port)
        .credentials(Credentials::new(
            cfg.smtp_user.clone(),
            cfg.smtp_pass.clone(),
        ))
        .build();

    transporter
        .send(email)
        .await
        .map_err(|e| format!("SMTP gonderimi basarisiz: {e}"))?;
    Ok(())
}

/// Kuyruk isleyici: pending mailleri SMTP'den (veya simulasyonla) gonderir.
pub fn spawn_email_worker(db: sqlx::SqlitePool, cfg: crate::config::Config) {
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(10)).await;
            let pending: Vec<(String, String, String, String)> = sqlx::query_as(
                "SELECT id, to_email, subject, body FROM email_outbox WHERE status = 'pending' LIMIT 20",
            )
            .fetch_all(&db)
            .await
            .unwrap_or_default();

            for (id, to, subject, body) in pending {
                match send_email(&cfg, &to, &subject, &body).await {
                    Ok(()) => {
                        let _ = sqlx::query(
                            "UPDATE email_outbox SET status = 'sent', sent_at = ?1 WHERE id = ?2",
                        )
                        .bind(util::now())
                        .bind(&id)
                        .execute(&db)
                        .await;
                    }
                    Err(e) => {
                        tracing::warn!("mail gonderilemedi ({to}): {e}");
                        let _ = sqlx::query(
                            "UPDATE email_outbox SET status = 'failed', sent_at = ?1 WHERE id = ?2",
                        )
                        .bind(util::now())
                        .bind(&id)
                        .execute(&db)
                        .await;
                    }
                }
            }
        }
    });
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/workspaces/{wid}/notifications",
            routing::get(list_mine),
        )
        .route(
            "/workspaces/{wid}/notifications/unread",
            routing::get(unread_count),
        )
        .route(
            "/workspaces/{wid}/notifications/read-all",
            routing::post(mark_all_read),
        )
        .route(
            "/workspaces/{wid}/notifications/{nid}/read",
            routing::post(mark_read),
        )
        .route(
            "/workspaces/{wid}/notifications/emails",
            routing::get(outbox),
        )
}
