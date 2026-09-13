use axum::extract::{Path, State};
use axum::{routing, Json, Router};
use serde::{Deserialize, Serialize};

use crate::auth::AuthUser;
use crate::error::{AppError, AppResult};
use crate::state::AppState;
use crate::util;
use crate::ws_ctx::WsCtx;

#[derive(Serialize, sqlx::FromRow)]
pub struct MemberRow {
    pub id: String,
    pub user_id: String,
    pub email: String,
    pub name: String,
    pub role_id: String,
    pub role_name: String,
    pub joined_at: String,
    pub scope_section_ids: Option<String>, // JSON array
}

#[derive(Deserialize)]
pub struct AddMemberReq {
    email: String,
    role_id: String,
}

#[derive(Deserialize)]
pub struct UpdateMemberRoleReq {
    role_id: String,
}

#[derive(Deserialize)]
pub struct SetScopesReq {
    /// Bos dizi / verilmezse = tum workspace erisimi; elemanlar section id
    #[serde(default)]
    section_ids: Vec<String>,
}

pub async fn list(
    State(state): State<AppState>,
    user: AuthUser,
    Path(wid): Path<String>,
) -> AppResult<Json<Vec<MemberRow>>> {
    WsCtx::load(&state.db, &user, &wid).await?;
    let rows = sqlx::query_as::<_, MemberRow>(
        "SELECT m.id, m.user_id, u.email, u.name, r.id AS role_id, r.name AS role_name,
                m.joined_at,
                (SELECT json_group_array(ms.section_id) FROM member_scopes ms WHERE ms.member_id = m.id) AS scope_section_ids
         FROM workspace_members m
         JOIN users u ON u.id = m.user_id
         JOIN roles r ON r.id = m.role_id
         WHERE m.workspace_id = ?1 AND m.archived_at IS NULL AND m.status = 'active'
         ORDER BY m.joined_at ASC",
    )
    .bind(&wid)
    .fetch_all(&state.db)
    .await?;
    Ok(Json(rows))
}

/// Email ile kayitli kullaniciyi direkt ekler (davet akisi 2. faz).
pub async fn add(
    State(state): State<AppState>,
    user: AuthUser,
    Path(wid): Path<String>,
    Json(req): Json<AddMemberReq>,
) -> AppResult<(axum::http::StatusCode, Json<MemberRow>)> {
    let ctx = WsCtx::load(&state.db, &user, &wid).await?;
    ctx.require("user.invite")?;

    let email = req.email.trim().to_lowercase();
    let target: Option<(String, i64)> =
        sqlx::query_as("SELECT id, is_active FROM users WHERE email = ?1")
            .bind(&email)
            .fetch_optional(&state.db)
            .await?;
    let (target_user_id, active) = target
        .ok_or_else(|| {
            AppError::BadRequest(
                "Bu e-posta ile kayitli kullanici yok. Kullanici once kayit olmali.".into(),
            )
        })?;
    if active != 1 {
        return Err(AppError::BadRequest("Kullanicinin hesabi pasif".into()));
    }

    // Rol ayni workspace'e mi ait ve Owner degil mi?
    let role: Option<(i64,)> =
        sqlx::query_as("SELECT is_system FROM roles WHERE id = ?1 AND workspace_id = ?2 AND name != 'Owner'")
            .bind(&req.role_id)
            .bind(&wid)
            .fetch_optional(&state.db)
            .await?;
    if role.is_none() {
        return Err(AppError::BadRequest("Gecersiz rol".into()));
    }

    let exists: Option<(String,)> = sqlx::query_as(
        "SELECT id FROM workspace_members WHERE workspace_id = ?1 AND user_id = ?2",
    )
    .bind(&wid)
    .bind(&target_user_id)
    .fetch_optional(&state.db)
    .await?;
    if exists.is_some() {
        return Err(AppError::Conflict("Kullanici zaten uye".into()));
    }

    let now = util::now();
    let mid = util::new_id();
    sqlx::query(
        "INSERT INTO workspace_members (id, workspace_id, user_id, role_id, status, joined_at, created_at)
         VALUES (?1, ?2, ?3, ?4, 'active', ?5, ?5)",
    )
    .bind(&mid)
    .bind(&wid)
    .bind(&target_user_id)
    .bind(&req.role_id)
    .bind(&now)
    .execute(&state.db)
    .await?;

    let row = sqlx::query_as::<_, MemberRow>(
        "SELECT m.id, m.user_id, u.email, u.name, r.id AS role_id, r.name AS role_name,
                m.joined_at,
                (SELECT json_group_array(ms.section_id) FROM member_scopes ms WHERE ms.member_id = m.id) AS scope_section_ids
         FROM workspace_members m
         JOIN users u ON u.id = m.user_id
         JOIN roles r ON r.id = m.role_id
         WHERE m.id = ?1",
    )
    .bind(&mid)
    .fetch_one(&state.db)
    .await?;
    crate::audit::audit(&state.db, &wid, Some(&user.0.id), "member.added", "workspace_member", &mid,
        serde_json::json!({ "email": email })).await;
    Ok((axum::http::StatusCode::CREATED, Json(row)))
}

pub async fn update_role(
    State(state): State<AppState>,
    user: AuthUser,
    Path((wid, mid)): Path<(String, String)>,
    Json(req): Json<UpdateMemberRoleReq>,
) -> AppResult<Json<MemberRow>> {
    let ctx = WsCtx::load(&state.db, &user, &wid).await?;
    ctx.require("role.manage")?;

    let target: Option<(String,)> =
        sqlx::query_as("SELECT r.name FROM workspace_members m JOIN roles r ON r.id = m.role_id WHERE m.id = ?1 AND m.workspace_id = ?2 AND m.archived_at IS NULL")
            .bind(&mid)
            .bind(&wid)
            .fetch_optional(&state.db)
            .await?;
    match target.as_ref().map(|(n,)| n.as_str()) {
        None => return Err(AppError::NotFound("Uye bulunamadi".into())),
        Some("Owner") => {
            return Err(AppError::Forbidden("Owner'un rolu degistirilemez".into()))
        }
        _ => {}
    }

    let role: Option<(i64,)> =
        sqlx::query_as("SELECT is_system FROM roles WHERE id = ?1 AND workspace_id = ?2 AND name != 'Owner'")
            .bind(&req.role_id)
            .bind(&wid)
            .fetch_optional(&state.db)
            .await?;
    if role.is_none() {
        return Err(AppError::BadRequest("Gecersiz rol".into()));
    }

    sqlx::query("UPDATE workspace_members SET role_id = ?1 WHERE id = ?2")
        .bind(&req.role_id)
        .bind(&mid)
        .execute(&state.db)
        .await?;

    let row = sqlx::query_as::<_, MemberRow>(
        "SELECT m.id, m.user_id, u.email, u.name, r.id AS role_id, r.name AS role_name,
                m.joined_at,
                (SELECT json_group_array(ms.section_id) FROM member_scopes ms WHERE ms.member_id = m.id) AS scope_section_ids
         FROM workspace_members m
         JOIN users u ON u.id = m.user_id
         JOIN roles r ON r.id = m.role_id
         WHERE m.id = ?1",
    )
    .bind(&mid)
    .fetch_one(&state.db)
    .await?;
    Ok(Json(row))
}

pub async fn remove(
    State(state): State<AppState>,
    user: AuthUser,
    Path((wid, mid)): Path<(String, String)>,
) -> AppResult<Json<serde_json::Value>> {
    let ctx = WsCtx::load(&state.db, &user, &wid).await?;
    // uye cikarma = user.invite; kendi kendini cikarma serbest
    if ctx.member_id != mid {
        ctx.require("user.invite")?;
    }

    let target: Option<(String, String)> = sqlx::query_as(
        "SELECT m.user_id, r.name FROM workspace_members m JOIN roles r ON r.id = m.role_id
         WHERE m.id = ?1 AND m.workspace_id = ?2 AND m.archived_at IS NULL",
    )
    .bind(&mid)
    .bind(&wid)
    .fetch_optional(&state.db)
    .await?;
    let (_, role_name) = target.ok_or_else(|| AppError::NotFound("Uye bulunamadi".into()))?;
    if role_name == "Owner" {
        return Err(AppError::Forbidden("Owner cikarilamaz".into()));
    }

    let now = util::now();
    let mut tx = state.db.begin().await?;
    sqlx::query(
        "UPDATE workspace_members SET status = 'removed', archived_at = ?1 WHERE id = ?2",
    )
    .bind(&now)
    .bind(&mid)
    .execute(&mut *tx)
    .await?;
    sqlx::query("DELETE FROM member_scopes WHERE member_id = ?1")
        .bind(&mid)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM team_members WHERE user_id = (SELECT user_id FROM workspace_members WHERE id = ?1) AND team_id IN (SELECT id FROM teams WHERE workspace_id = ?2)")
        .bind(&mid)
        .bind(&wid)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    crate::audit::audit(&state.db, &wid, Some(&user.0.id), "member.removed", "workspace_member", &mid,
        serde_json::json!({})).await;
    Ok(Json(serde_json::json!({ "ok": true })))
}

/// Uyenin section kapsam erisimini tamamen degistirir.
pub async fn set_scopes(
    State(state): State<AppState>,
    user: AuthUser,
    Path((wid, mid)): Path<(String, String)>,
    Json(req): Json<SetScopesReq>,
) -> AppResult<Json<serde_json::Value>> {
    let ctx = WsCtx::load(&state.db, &user, &wid).await?;
    ctx.require("role.manage")?;

    let target: Option<(String,)> = sqlx::query_as(
        "SELECT r.name FROM workspace_members m JOIN roles r ON r.id = m.role_id
         WHERE m.id = ?1 AND m.workspace_id = ?2 AND m.archived_at IS NULL",
    )
    .bind(&mid)
    .bind(&wid)
    .fetch_optional(&state.db)
    .await?;
    match target.as_ref().map(|(n,)| n.as_str()) {
        None => return Err(AppError::NotFound("Uye bulunamadi".into())),
        Some("Owner") => {
            return Err(AppError::Forbidden(
                "Owner icin kapsam kisitlamasi yapilamaz".into(),
            ))
        }
        _ => {}
    }

    // Section'lar ayni workspace'e ait olmali
    for sid in &req.section_ids {
        let ok: Option<(String,)> = sqlx::query_as(
            "SELECT id FROM sections WHERE id = ?1 AND workspace_id = ?2 AND archived_at IS NULL",
        )
        .bind(sid)
        .bind(&wid)
        .fetch_optional(&state.db)
        .await?;
        if ok.is_none() {
            return Err(AppError::BadRequest("Gecersiz section id kapsamda".into()));
        }
    }

    let now = util::now();
    let mut tx = state.db.begin().await?;
    sqlx::query("DELETE FROM member_scopes WHERE member_id = ?1")
        .bind(&mid)
        .execute(&mut *tx)
        .await?;
    for sid in &req.section_ids {
        sqlx::query(
            "INSERT INTO member_scopes (id, member_id, section_id, created_at) VALUES (?1, ?2, ?3, ?4)",
        )
        .bind(util::new_id())
        .bind(&mid)
        .bind(sid)
        .bind(&now)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/workspaces/{wid}/members", routing::get(list).post(add))
        .route(
            "/workspaces/{wid}/members/{mid}",
            routing::patch(update_role).delete(remove),
        )
        .route("/workspaces/{wid}/members/{mid}/scopes", routing::put(set_scopes))
}
