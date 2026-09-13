use axum::extract::{Path, State};
use axum::{routing, Json, Router};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

use crate::auth::AuthUser;
use crate::error::{AppError, AppResult};
use crate::state::AppState;
use crate::util;
use crate::ws_ctx::WsCtx;

#[derive(Serialize, sqlx::FromRow)]
pub struct WorkspaceSummary {
    pub id: String,
    pub name: String,
    pub slug: String,
    pub status: String,
    pub role_name: Option<String>,
}

#[derive(Serialize, sqlx::FromRow)]
pub struct WorkspaceRow {
    pub id: String,
    pub name: String,
    pub slug: String,
    pub owner_id: String,
    pub status: String,
    pub created_at: String,
    pub archived_at: Option<String>,
}

#[derive(Deserialize)]
pub struct CreateWorkspaceReq {
    name: String,
}

#[derive(Deserialize)]
pub struct UpdateWorkspaceReq {
    name: String,
}

/// Kullanicinin bu workspace'teki rolu ve izinleri (rol bazli UI icin).
#[derive(Serialize)]
pub struct MeOut {
    pub role_name: String,
    pub is_owner: bool,
    pub permissions: Vec<String>,
}

pub async fn me(
    State(state): State<AppState>,
    user: AuthUser,
    Path(wid): Path<String>,
) -> AppResult<Json<MeOut>> {
    let ctx = WsCtx::load(&state.db, &user, &wid).await?;
    let mut perms: Vec<String> = ctx.permissions.iter().cloned().collect();
    perms.sort();
    Ok(Json(MeOut {
        role_name: ctx.role_name,
        is_owner: ctx.is_owner,
        permissions: perms,
    }))
}

async fn unique_slug(db: &SqlitePool, base: &str) -> AppResult<String> {
    let mut slug = base.to_string();
    let mut n = 1;
    loop {
        let exists: Option<(String,)> =
            sqlx::query_as("SELECT id FROM workspaces WHERE slug = ?1")
                .bind(&slug)
                .fetch_optional(db)
                .await?;
        if exists.is_none() {
            return Ok(slug);
        }
        n += 1;
        slug = format!("{base}-{n}");
    }
}

/// Workspace + varsayilan sistem rolleri + Owner uyeligi tek transaction.
async fn create_workspace_tx(db: &SqlitePool, name: &str, owner_user_id: &str) -> AppResult<String> {
    let ws_id = util::new_id();
    let now = util::now();
    let slug = unique_slug(db, &util::slugify(name)).await?;

    let mut tx = db.begin().await?;

    sqlx::query(
        "INSERT INTO workspaces (id, name, slug, owner_id, status, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, 'active', ?5, ?5)",
    )
    .bind(&ws_id)
    .bind(name)
    .bind(&slug)
    .bind(owner_user_id)
    .bind(&now)
    .execute(&mut *tx)
    .await?;

    // Sistem rollerini olustur ve izinleri ata
    let mut role_ids: Vec<(String, &str)> = Vec::new();
    for def in crate::permissions::SYSTEM_ROLES {
        let role_id = util::new_id();
        sqlx::query(
            "INSERT INTO roles (id, workspace_id, name, description, is_system, created_at)
             VALUES (?1, ?2, ?3, ?4, 1, ?5)",
        )
        .bind(&role_id)
        .bind(&ws_id)
        .bind(def.name)
        .bind(def.description)
        .bind(&now)
        .execute(&mut *tx)
        .await?;

        for perm in def.permissions {
            sqlx::query(
                "INSERT INTO role_permissions (role_id, permission_id)
                 SELECT ?1, id FROM permissions WHERE key = ?2",
            )
            .bind(&role_id)
            .bind(perm)
            .execute(&mut *tx)
            .await?;
        }
        role_ids.push((role_id, def.name));
    }

    // Owner uyeligi
    let owner_role_id = role_ids
        .iter()
        .find(|(_, n)| *n == "Owner")
        .map(|(id, _)| id.clone())
        .expect("Owner rolu olusmali");
    sqlx::query(
        "INSERT INTO workspace_members (id, workspace_id, user_id, role_id, status, joined_at, created_at)
         VALUES (?1, ?2, ?3, ?4, 'active', ?5, ?5)",
    )
    .bind(util::new_id())
    .bind(&ws_id)
    .bind(owner_user_id)
    .bind(&owner_role_id)
    .bind(&now)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(ws_id)
}

pub async fn list_mine(
    State(state): State<AppState>,
    user: AuthUser,
) -> AppResult<Json<Vec<WorkspaceSummary>>> {
    let rows = sqlx::query_as::<_, WorkspaceSummary>(
        "SELECT w.id, w.name, w.slug, w.status, r.name AS role_name
         FROM workspace_members m
         JOIN workspaces w ON w.id = m.workspace_id
         JOIN roles r ON r.id = m.role_id
         WHERE m.user_id = ?1 AND m.archived_at IS NULL AND m.status = 'active'
         ORDER BY w.created_at DESC",
    )
    .bind(&user.0.id)
    .fetch_all(&state.db)
    .await?;
    Ok(Json(rows))
}

pub async fn create(
    State(state): State<AppState>,
    user: AuthUser,
    Json(req): Json<CreateWorkspaceReq>,
) -> AppResult<(axum::http::StatusCode, Json<WorkspaceRow>)> {
    let name = req.name.trim().to_string();
    if name.is_empty() || name.len() > 80 {
        return Err(AppError::BadRequest(
            "Workspace adi 1-80 karakter olmali".into(),
        ));
    }
    let ws_id = create_workspace_tx(&state.db, &name, &user.0.id).await?;
    let row = sqlx::query_as::<_, WorkspaceRow>(
        "SELECT id, name, slug, owner_id, status, created_at, archived_at FROM workspaces WHERE id = ?1",
    )
    .bind(&ws_id)
    .fetch_one(&state.db)
    .await?;
    Ok((axum::http::StatusCode::CREATED, Json(row)))
}

pub async fn get_one(
    State(state): State<AppState>,
    user: AuthUser,
    Path(wid): Path<String>,
) -> AppResult<Json<WorkspaceSummary>> {
    let row = sqlx::query_as::<_, WorkspaceSummary>(
        "SELECT w.id, w.name, w.slug, w.status, r.name AS role_name
         FROM workspace_members m
         JOIN workspaces w ON w.id = m.workspace_id
         JOIN roles r ON r.id = m.role_id
         WHERE m.workspace_id = ?1 AND m.user_id = ?2 AND m.archived_at IS NULL",
    )
    .bind(&wid)
    .bind(&user.0.id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound("Workspace bulunamadi".into()))?;
    Ok(Json(row))
}

pub async fn update(
    State(state): State<AppState>,
    user: AuthUser,
    Path(wid): Path<String>,
    Json(req): Json<UpdateWorkspaceReq>,
) -> AppResult<Json<WorkspaceRow>> {
    let ctx = WsCtx::load(&state.db, &user, &wid).await?;
    ctx.require("workspace.update")?;
    let name = req.name.trim().to_string();
    if name.is_empty() || name.len() > 80 {
        return Err(AppError::BadRequest(
            "Workspace adi 1-80 karakter olmali".into(),
        ));
    }
    sqlx::query("UPDATE workspaces SET name = ?1, updated_at = ?2 WHERE id = ?3")
        .bind(&name)
        .bind(util::now())
        .bind(&wid)
        .execute(&state.db)
        .await?;
    let row = sqlx::query_as::<_, WorkspaceRow>(
        "SELECT id, name, slug, owner_id, status, created_at, archived_at FROM workspaces WHERE id = ?1",
    )
    .bind(&wid)
    .fetch_one(&state.db)
    .await?;
    Ok(Json(row))
}

pub async fn archive(
    State(state): State<AppState>,
    user: AuthUser,
    Path(wid): Path<String>,
) -> AppResult<Json<serde_json::Value>> {
    let ctx = WsCtx::load(&state.db, &user, &wid).await?;
    if !ctx.is_owner {
        return Err(AppError::Forbidden(
            "Sadece workspace sahibi arsivleyebilir".into(),
        ));
    }
    let now = util::now();
    sqlx::query(
        "UPDATE workspaces SET status = 'archived', archived_at = ?1, updated_at = ?1 WHERE id = ?2",
    )
    .bind(&now)
    .bind(&wid)
    .execute(&state.db)
    .await?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/workspaces", routing::get(list_mine).post(create))
        .route("/workspaces/{wid}", routing::get(get_one).patch(update))
        .route("/workspaces/{wid}/me", routing::get(me))
        .route("/workspaces/{wid}/archive", routing::post(archive))
}
