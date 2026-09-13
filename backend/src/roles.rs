use axum::extract::{Path, State};
use axum::{routing, Json, Router};
use serde::{Deserialize, Serialize};

use crate::auth::AuthUser;
use crate::error::{AppError, AppResult};
use crate::permissions;
use crate::state::AppState;
use crate::util;
use crate::ws_ctx::WsCtx;

#[derive(Serialize, sqlx::FromRow)]
pub struct RoleRow {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub is_system: bool,
    pub permission_keys: Option<String>, // JSON array
}

#[derive(Deserialize)]
pub struct CreateRoleReq {
    name: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    permission_keys: Vec<String>,
}

#[derive(Deserialize)]
pub struct UpdateRoleReq {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    permission_keys: Option<Vec<String>>,
}

const ROLE_SELECT: &str = "SELECT r.id, r.name, r.description, r.is_system = 1 AS is_system,
    (SELECT json_group_array(p.key) FROM role_permissions rp JOIN permissions p ON p.id = rp.permission_id WHERE rp.role_id = r.id) AS permission_keys
    FROM roles r";

pub async fn list(
    State(state): State<AppState>,
    user: AuthUser,
    Path(wid): Path<String>,
) -> AppResult<Json<Vec<RoleRow>>> {
    WsCtx::load(&state.db, &user, &wid).await?;
    let rows = sqlx::query_as::<_, RoleRow>(&format!(
        "{ROLE_SELECT} WHERE r.workspace_id = ?1 ORDER BY CASE WHEN r.is_system = 1 THEN 0 ELSE 1 END, r.name"
    ))
    .bind(&wid)
    .fetch_all(&state.db)
    .await?;
    Ok(Json(rows))
}

pub async fn create(
    State(state): State<AppState>,
    user: AuthUser,
    Path(wid): Path<String>,
    Json(req): Json<CreateRoleReq>,
) -> AppResult<(axum::http::StatusCode, Json<RoleRow>)> {
    let ctx = WsCtx::load(&state.db, &user, &wid).await?;
    ctx.require("role.manage")?;

    let name = req.name.trim().to_string();
    if name.is_empty() || name.len() > 40 {
        return Err(AppError::BadRequest("Rol adi 1-40 karakter olmali".into()));
    }
    for k in &req.permission_keys {
        if !permissions::is_valid_permission(k) {
            return Err(AppError::BadRequest(format!("Bilinmeyen izin: {k}")));
        }
    }
    let exists: Option<(String,)> =
        sqlx::query_as("SELECT id FROM roles WHERE workspace_id = ?1 AND name = ?2")
            .bind(&wid)
            .bind(&name)
            .fetch_optional(&state.db)
            .await?;
    if exists.is_some() {
        return Err(AppError::Conflict("Bu isimde rol zaten var".into()));
    }

    let role_id = util::new_id();
    let now = util::now();
    let mut tx = state.db.begin().await?;
    sqlx::query(
        "INSERT INTO roles (id, workspace_id, name, description, is_system, created_at)
         VALUES (?1, ?2, ?3, ?4, 0, ?5)",
    )
    .bind(&role_id)
    .bind(&wid)
    .bind(&name)
    .bind(&req.description)
    .bind(&now)
    .execute(&mut *tx)
    .await?;
    for k in &req.permission_keys {
        sqlx::query(
            "INSERT INTO role_permissions (role_id, permission_id) SELECT ?1, id FROM permissions WHERE key = ?2",
        )
        .bind(&role_id)
        .bind(k)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;

    let row = sqlx::query_as::<_, RoleRow>(&format!("{ROLE_SELECT} WHERE r.id = ?1"))
        .bind(&role_id)
        .fetch_one(&state.db)
        .await?;
    Ok((axum::http::StatusCode::CREATED, Json(row)))
}

/// Sistem rollerinin izinleri duzenlenebilir (Owner haric); isimleri sabit.
/// Ozel rollerde isim de duzenlenebilir.
pub async fn update(
    State(state): State<AppState>,
    user: AuthUser,
    Path((wid, rid)): Path<(String, String)>,
    Json(req): Json<UpdateRoleReq>,
) -> AppResult<Json<RoleRow>> {
    let ctx = WsCtx::load(&state.db, &user, &wid).await?;
    ctx.require("role.manage")?;

    let role: Option<(String, i64)> =
        sqlx::query_as("SELECT name, is_system FROM roles WHERE id = ?1 AND workspace_id = ?2")
            .bind(&rid)
            .bind(&wid)
            .fetch_optional(&state.db)
            .await?;
    let (role_name, is_system) =
        role.ok_or_else(|| AppError::NotFound("Rol bulunamadi".into()))?;
    if role_name == "Owner" {
        return Err(AppError::Forbidden("Owner rolu duzenlenemez".into()));
    }

    if let Some(keys) = &req.permission_keys {
        for k in keys {
            if !permissions::is_valid_permission(k) {
                return Err(AppError::BadRequest(format!("Bilinmeyen izin: {k}")));
            }
        }
    }
    if is_system == 1 {
        if req.name.is_some() {
            return Err(AppError::Forbidden(
                "Sistem rolunun adi degistirilemez".into(),
            ));
        }
    } else if let Some(n) = &req.name {
        let n = n.trim();
        if n.is_empty() || n.len() > 40 {
            return Err(AppError::BadRequest("Rol adi 1-40 karakter olmali".into()));
        }
    }

    let now = util::now();
    let mut tx = state.db.begin().await?;

    if let Some(n) = &req.name {
        sqlx::query("UPDATE roles SET name = ?1 WHERE id = ?2")
            .bind(n.trim())
            .bind(&rid)
            .execute(&mut *tx)
            .await?;
    }
    if let Some(d) = &req.description {
        sqlx::query("UPDATE roles SET description = ?1 WHERE id = ?2")
            .bind(d)
            .bind(&rid)
            .execute(&mut *tx)
            .await?;
    }
    if let Some(keys) = &req.permission_keys {
        sqlx::query("DELETE FROM role_permissions WHERE role_id = ?1")
            .bind(&rid)
            .execute(&mut *tx)
            .await?;
        for k in keys {
            sqlx::query(
                "INSERT INTO role_permissions (role_id, permission_id) SELECT ?1, id FROM permissions WHERE key = ?2",
            )
            .bind(&rid)
            .bind(k)
            .execute(&mut *tx)
            .await?;
        }
    }
    let _ = now;
    tx.commit().await?;

    let row = sqlx::query_as::<_, RoleRow>(&format!("{ROLE_SELECT} WHERE r.id = ?1"))
        .bind(&rid)
        .fetch_one(&state.db)
        .await?;
    Ok(Json(row))
}

/// Tum sistem izinlerinin statik listesi (rol editoru icin).
pub async fn list_permissions() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "permissions": permissions::ALL_PERMISSIONS
            .iter()
            .map(|(k, d)| serde_json::json!({ "key": k, "description": d }))
            .collect::<Vec<_>>()
    }))
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/workspaces/{wid}/roles", routing::get(list).post(create))
        .route("/workspaces/{wid}/roles/{rid}", routing::patch(update))
        .route("/permissions", routing::get(list_permissions))
}
