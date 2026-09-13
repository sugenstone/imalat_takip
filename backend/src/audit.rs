//! Faz 8: Audit log (yol haritasi 35) — append-only aktivite kayitlari.
//! Kayitlarin degistirilmesi/silinmesi icin endpoint YOKTUR.

use axum::extract::{Path, Query, State};
use axum::{routing, Json, Router};
use serde::{Deserialize, Serialize};

use crate::auth::AuthUser;
use crate::error::{AppError, AppResult};
use crate::state::AppState;
use crate::util;
use crate::ws_ctx::WsCtx;

/// Kritik islem icin audit kaydi yaz. Basarisizlik ana akisi bozmamali.
pub async fn audit(
    db: &sqlx::SqlitePool,
    wid: &str,
    actor: Option<&str>,
    action: &str,
    entity_type: &str,
    entity_id: &str,
    metadata: serde_json::Value,
) {
    let res = sqlx::query(
        "INSERT INTO audit_logs (id, workspace_id, actor_user_id, action, entity_type, entity_id, metadata_json, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
    )
    .bind(util::new_id())
    .bind(wid)
    .bind(actor)
    .bind(action)
    .bind(entity_type)
    .bind(entity_id)
    .bind(metadata.to_string())
    .bind(util::now())
    .execute(db)
    .await;
    if let Err(e) = res {
        tracing::warn!("audit yazilamadi ({action}): {e}");
    }
}

#[derive(Serialize, sqlx::FromRow)]
pub struct AuditRow {
    pub id: String,
    pub action: String,
    pub entity_type: Option<String>,
    pub entity_id: Option<String>,
    pub metadata_json: Option<String>,
    pub created_at: String,
    pub actor_name: Option<String>,
}

#[derive(Deserialize)]
pub struct AuditQuery {
    #[serde(default)]
    entity_type: Option<String>,
    #[serde(default)]
    entity_id: Option<String>,
    #[serde(default = "default_limit")]
    limit: i64,
}

fn default_limit() -> i64 {
    100
}

/// Aktivite gecmisi — yalnizca Owner/Admin (role.manage yetkisi) goruntuler.
pub async fn list(
    State(state): State<AppState>,
    user: AuthUser,
    Path(wid): axum::extract::Path<String>,
    Query(q): Query<AuditQuery>,
) -> AppResult<Json<Vec<AuditRow>>> {
    let ctx = WsCtx::load(&state.db, &user, &wid).await?;
    if !ctx.is_owner && !ctx.permissions.contains("role.manage") {
        return Err(AppError::Forbidden(
            "Aktivite gecmisini yoneticiler goruntulebilir".into(),
        ));
    }

    let limit = q.limit.clamp(1, 200);
    let rows = sqlx::query_as::<_, AuditRow>(
        "SELECT a.id, a.action, a.entity_type, a.entity_id, a.metadata_json, a.created_at,
                (SELECT u.name FROM users u WHERE u.id = a.actor_user_id) AS actor_name
         FROM audit_logs a
         WHERE a.workspace_id = ?1
           AND (?2 IS NULL OR a.entity_type = ?2)
           AND (?3 IS NULL OR a.entity_id = ?3)
         ORDER BY a.created_at DESC
         LIMIT ?4",
    )
    .bind(&wid)
    .bind(&q.entity_type)
    .bind(&q.entity_id)
    .bind(limit)
    .fetch_all(&state.db)
    .await?;
    Ok(Json(rows))
}

pub fn router() -> Router<AppState> {
    Router::new().route("/workspaces/{wid}/audit", routing::get(list))
}
