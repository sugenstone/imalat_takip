use axum::extract::{Path, State};
use axum::{routing, Json, Router};
use serde::{Deserialize, Serialize};

use crate::auth::AuthUser;
use crate::error::{AppError, AppResult};
use crate::state::AppState;
use crate::util;
use crate::ws_ctx::WsCtx;

#[derive(Serialize, sqlx::FromRow)]
pub struct TeamRow {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub member_count: i64,
    pub member_ids: Option<String>, // JSON array
}

#[derive(Deserialize)]
pub struct CreateTeamReq {
    name: String,
    #[serde(default)]
    description: Option<String>,
}

#[derive(Deserialize)]
pub struct UpdateTeamReq {
    name: String,
    #[serde(default)]
    description: Option<String>,
}

#[derive(Deserialize)]
pub struct AddTeamMemberReq {
    user_id: String,
}

const TEAM_SELECT: &str = "SELECT t.id, t.name, t.description,
    (SELECT COUNT(*) FROM team_members tm WHERE tm.team_id = t.id) AS member_count,
    (SELECT json_group_array(tm.user_id) FROM team_members tm WHERE tm.team_id = t.id) AS member_ids
    FROM teams t";

pub async fn list(
    State(state): State<AppState>,
    user: AuthUser,
    Path(wid): Path<String>,
) -> AppResult<Json<Vec<TeamRow>>> {
    WsCtx::load(&state.db, &user, &wid).await?;
    let rows = sqlx::query_as::<_, TeamRow>(&format!(
        "{TEAM_SELECT} WHERE t.workspace_id = ?1 AND t.archived_at IS NULL ORDER BY t.name"
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
    Json(req): Json<CreateTeamReq>,
) -> AppResult<(axum::http::StatusCode, Json<TeamRow>)> {
    let ctx = WsCtx::load(&state.db, &user, &wid).await?;
    ctx.require("team.manage")?;

    let name = req.name.trim().to_string();
    if name.is_empty() || name.len() > 60 {
        return Err(AppError::BadRequest("Takim adi 1-60 karakter olmali".into()));
    }
    let exists: Option<(String,)> =
        sqlx::query_as("SELECT id FROM teams WHERE workspace_id = ?1 AND name = ?2 AND archived_at IS NULL")
            .bind(&wid)
            .bind(&name)
            .fetch_optional(&state.db)
            .await?;
    if exists.is_some() {
        return Err(AppError::Conflict("Bu isimde takim zaten var".into()));
    }

    let id = util::new_id();
    let now = util::now();
    sqlx::query(
        "INSERT INTO teams (id, workspace_id, name, description, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?5)",
    )
    .bind(&id)
    .bind(&wid)
    .bind(&name)
    .bind(&req.description)
    .bind(&now)
    .execute(&state.db)
    .await?;

    let row = sqlx::query_as::<_, TeamRow>(&format!("{TEAM_SELECT} WHERE t.id = ?1"))
        .bind(&id)
        .fetch_one(&state.db)
        .await?;
    Ok((axum::http::StatusCode::CREATED, Json(row)))
}

pub async fn update(
    State(state): State<AppState>,
    user: AuthUser,
    Path((wid, tid)): Path<(String, String)>,
    Json(req): Json<UpdateTeamReq>,
) -> AppResult<Json<TeamRow>> {
    let ctx = WsCtx::load(&state.db, &user, &wid).await?;
    ctx.require("team.manage")?;

    let name = req.name.trim().to_string();
    if name.is_empty() || name.len() > 60 {
        return Err(AppError::BadRequest("Takim adi 1-60 karakter olmali".into()));
    }
    let res = sqlx::query("UPDATE teams SET name = ?1, description = ?2, updated_at = ?3 WHERE id = ?4 AND workspace_id = ?5 AND archived_at IS NULL")
        .bind(&name)
        .bind(&req.description)
        .bind(util::now())
        .bind(&tid)
        .bind(&wid)
        .execute(&state.db)
        .await?;
    if res.rows_affected() == 0 {
        return Err(AppError::NotFound("Takim bulunamadi".into()));
    }

    let row = sqlx::query_as::<_, TeamRow>(&format!("{TEAM_SELECT} WHERE t.id = ?1"))
        .bind(&tid)
        .fetch_one(&state.db)
        .await?;
    Ok(Json(row))
}

pub async fn archive(
    State(state): State<AppState>,
    user: AuthUser,
    Path((wid, tid)): Path<(String, String)>,
) -> AppResult<Json<serde_json::Value>> {
    let ctx = WsCtx::load(&state.db, &user, &wid).await?;
    ctx.require("team.manage")?;

    let now = util::now();
    let res = sqlx::query(
        "UPDATE teams SET archived_at = ?1, updated_at = ?1 WHERE id = ?2 AND workspace_id = ?3 AND archived_at IS NULL",
    )
    .bind(&now)
    .bind(&tid)
    .bind(&wid)
    .execute(&state.db)
    .await?;
    if res.rows_affected() == 0 {
        return Err(AppError::NotFound("Takim bulunamadi".into()));
    }
    Ok(Json(serde_json::json!({ "ok": true })))
}

/// Takima uye ekleme - sadece workspace uyesi eklenebilir.
pub async fn add_member(
    State(state): State<AppState>,
    user: AuthUser,
    Path((wid, tid)): Path<(String, String)>,
    Json(req): Json<AddTeamMemberReq>,
) -> AppResult<Json<serde_json::Value>> {
    let ctx = WsCtx::load(&state.db, &user, &wid).await?;
    ctx.require("team.manage")?;

    let team: Option<(String,)> = sqlx::query_as(
        "SELECT id FROM teams WHERE id = ?1 AND workspace_id = ?2 AND archived_at IS NULL",
    )
    .bind(&tid)
    .bind(&wid)
    .fetch_optional(&state.db)
    .await?;
    if team.is_none() {
        return Err(AppError::NotFound("Takim bulunamadi".into()));
    }

    let member: Option<(String,)> = sqlx::query_as(
        "SELECT id FROM workspace_members WHERE workspace_id = ?1 AND user_id = ?2 AND archived_at IS NULL AND status = 'active'",
    )
    .bind(&wid)
    .bind(&req.user_id)
    .fetch_optional(&state.db)
    .await?;
    if member.is_none() {
        return Err(AppError::BadRequest("Kullanici bu workspace'in uyesi degil".into()));
    }

    sqlx::query(
        "INSERT OR IGNORE INTO team_members (team_id, user_id, added_at) VALUES (?1, ?2, ?3)",
    )
    .bind(&tid)
    .bind(&req.user_id)
    .bind(util::now())
    .execute(&state.db)
    .await?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

pub async fn remove_member(
    State(state): State<AppState>,
    user: AuthUser,
    Path((wid, tid, uid)): Path<(String, String, String)>,
) -> AppResult<Json<serde_json::Value>> {
    let ctx = WsCtx::load(&state.db, &user, &wid).await?;
    ctx.require("team.manage")?;

    sqlx::query("DELETE FROM team_members WHERE team_id = ?1 AND user_id = ?2")
        .bind(&tid)
        .bind(&uid)
        .execute(&state.db)
        .await?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/workspaces/{wid}/teams", routing::get(list).post(create))
        .route(
            "/workspaces/{wid}/teams/{tid}",
            routing::patch(update).delete(archive),
        )
        .route(
            "/workspaces/{wid}/teams/{tid}/members",
            routing::post(add_member),
        )
        .route(
            "/workspaces/{wid}/teams/{tid}/members/{uid}",
            routing::delete(remove_member),
        )
}
