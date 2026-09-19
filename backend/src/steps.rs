//! Adim Havuzu (step pool): workspace genelinde yeniden kullanilabilir surec
//! adimi tanimlari — ad + varsayilan sorumlu + onay kurali bir kez tanimlanir,
//! surec gruplari (workflows.rs /step-groups) buradan secerek kurulur.

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
pub struct StepRow {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub default_assignee_type: Option<String>,
    pub default_assignee_id: Option<String>,
    pub requires_approval: bool,
    pub approver_role_id: Option<String>,
    pub sort_order: i64,
}

#[derive(Deserialize, Default)]
pub struct StepReq {
    pub name: Option<String>,
    pub description: Option<String>,
    /// {type: user|team, id} | null — null temizler
    pub default_assignee: Option<serde_json::Value>,
    #[serde(default)]
    pub requires_approval: bool,
    pub approver_role_id: Option<String>,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/workspaces/{wid}/steps", routing::get(list).post(create))
        .route("/workspaces/{wid}/steps/{sid}", routing::patch(update))
        .route("/workspaces/{wid}/steps/{sid}/archive", routing::post(archive))
}

pub async fn list(
    State(state): State<AppState>,
    user: AuthUser,
    Path(wid): Path<String>,
) -> AppResult<Json<Vec<StepRow>>> {
    WsCtx::load(&state.db, &user, &wid).await?;
    let rows = sqlx::query_as::<_, StepRow>(
        "SELECT id, name, description, default_assignee_type, default_assignee_id,
                requires_approval, approver_role_id, sort_order
         FROM step_definitions
         WHERE workspace_id = ?1 AND archived_at IS NULL
         ORDER BY sort_order, name",
    )
    .bind(&wid)
    .fetch_all(&state.db)
    .await?;
    Ok(Json(rows))
}

/// Varsayilan atanan + onay rolu dogrulamasi (update_node desenleriyle ayni).
async fn validate_fields(
    db: &SqlitePool,
    wid: &str,
    req: &StepReq,
) -> AppResult<(Option<String>, Option<String>, Option<String>)> {
    // Varsayilan atanan
    let (mut atype, mut aid) = (None, None);
    if let Some(assignee) = &req.default_assignee {
        if !assignee.is_null() {
            let t = assignee.get("type").and_then(|v| v.as_str()).unwrap_or("");
            let i = assignee.get("id").and_then(|v| v.as_str()).unwrap_or("");
            if !matches!(t, "user" | "team") || i.is_empty() {
                return Err(AppError::BadRequest(
                    "default_assignee: {type: user|team, id} gerekli".into(),
                ));
            }
            if t == "user" {
                let ok: Option<(String,)> = sqlx::query_as(
                    "SELECT id FROM workspace_members WHERE workspace_id = ?1 AND user_id = ?2 AND status = 'active' AND archived_at IS NULL",
                )
                .bind(wid)
                .bind(i)
                .fetch_optional(db)
                .await?;
                if ok.is_none() {
                    return Err(AppError::BadRequest(
                        "Kullanici bu workspace'in uyesi degil".into(),
                    ));
                }
            } else {
                let ok: Option<(String,)> = sqlx::query_as(
                    "SELECT id FROM teams WHERE id = ?1 AND workspace_id = ?2 AND archived_at IS NULL",
                )
                .bind(i)
                .bind(wid)
                .fetch_optional(db)
                .await?;
                if ok.is_none() {
                    return Err(AppError::BadRequest("Takim bulunamadi".into()));
                }
            }
            atype = Some(t.to_string());
            aid = Some(i.to_string());
        }
    }

    // Onay kurali: rol verilmissa workspace'te olmali
    let mut role = None;
    if req.requires_approval {
        if let Some(rid) = &req.approver_role_id {
            let ok: Option<(String,)> =
                sqlx::query_as("SELECT id FROM roles WHERE id = ?1 AND workspace_id = ?2")
                    .bind(rid)
                    .bind(wid)
                    .fetch_optional(db)
                    .await?;
            if ok.is_none() {
                return Err(AppError::BadRequest("Gecersiz onay rolu".into()));
            }
            role = Some(rid.clone());
        }
    }

    Ok((atype, aid, role))
}

pub async fn create(
    State(state): State<AppState>,
    user: AuthUser,
    Path(wid): Path<String>,
    Json(req): Json<StepReq>,
) -> AppResult<(axum::http::StatusCode, Json<StepRow>)> {
    let ctx = WsCtx::load(&state.db, &user, &wid).await?;
    ctx.require("workflow.create")?;

    let name = req.name.clone().unwrap_or_default().trim().to_string();
    if name.is_empty() || name.len() > 80 {
        return Err(AppError::BadRequest("Adim adi 1-80 karakter olmali".into()));
    }
    let (atype, aid, role) = validate_fields(&state.db, &wid, &req).await?;

    let dup: Option<(String,)> = sqlx::query_as(
        "SELECT id FROM step_definitions WHERE workspace_id = ?1 AND name = ?2 AND archived_at IS NULL",
    )
    .bind(&wid)
    .bind(&name)
    .fetch_optional(&state.db)
    .await?;
    if dup.is_some() {
        return Err(AppError::Conflict("Bu adda bir adim zaten var".into()));
    }

    let (max_sort,): (i64,) = sqlx::query_as(
        "SELECT COALESCE(MAX(sort_order), 0) FROM step_definitions WHERE workspace_id = ?1",
    )
    .bind(&wid)
    .fetch_one(&state.db)
    .await?;

    let id = util::new_id();
    sqlx::query(
        "INSERT INTO step_definitions
         (id, workspace_id, name, description, default_assignee_type, default_assignee_id,
          requires_approval, approver_role_id, sort_order, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
    )
    .bind(&id)
    .bind(&wid)
    .bind(&name)
    .bind(&req.description)
    .bind(&atype)
    .bind(&aid)
    .bind(req.requires_approval)
    .bind(&role)
    .bind(max_sort + 1)
    .bind(util::now())
    .execute(&state.db)
    .await?;

    crate::audit::audit(&state.db, &wid, Some(&user.0.id), "step.created", "step_definition", &id,
        serde_json::json!({ "name": name })).await;

    let row = sqlx::query_as::<_, StepRow>(
        "SELECT id, name, description, default_assignee_type, default_assignee_id,
                requires_approval, approver_role_id, sort_order
         FROM step_definitions WHERE id = ?1",
    )
    .bind(&id)
    .fetch_one(&state.db)
    .await?;
    Ok((axum::http::StatusCode::CREATED, Json(row)))
}

pub async fn update(
    State(state): State<AppState>,
    user: AuthUser,
    Path((wid, sid)): Path<(String, String)>,
    Json(req): Json<StepReq>,
) -> AppResult<Json<serde_json::Value>> {
    let ctx = WsCtx::load(&state.db, &user, &wid).await?;
    ctx.require("workflow.create")?;

    let existing: Option<(String,)> = sqlx::query_as(
        "SELECT id FROM step_definitions WHERE id = ?1 AND workspace_id = ?2 AND archived_at IS NULL",
    )
    .bind(&sid)
    .bind(&wid)
    .fetch_optional(&state.db)
    .await?;
    if existing.is_none() {
        return Err(AppError::NotFound("Adim bulunamadi".into()));
    }

    if let Some(n) = &req.name {
        let n = n.trim().to_string();
        if n.is_empty() || n.len() > 80 {
            return Err(AppError::BadRequest("Adim adi 1-80 karakter olmali".into()));
        }
        let dup: Option<(String,)> = sqlx::query_as(
            "SELECT id FROM step_definitions WHERE workspace_id = ?1 AND name = ?2 AND id != ?3 AND archived_at IS NULL",
        )
        .bind(&wid)
        .bind(&n)
        .bind(&sid)
        .fetch_optional(&state.db)
        .await?;
        if dup.is_some() {
            return Err(AppError::Conflict("Bu adda bir adim zaten var".into()));
        }
        sqlx::query("UPDATE step_definitions SET name = ?1 WHERE id = ?2")
            .bind(&n)
            .bind(&sid)
            .execute(&state.db)
            .await?;
    }
    if let Some(d) = &req.description {
        sqlx::query("UPDATE step_definitions SET description = ?1 WHERE id = ?2")
            .bind(d)
            .bind(&sid)
            .execute(&state.db)
            .await?;
    }

    let (atype, aid, role) = validate_fields(&state.db, &wid, &req).await?;
    if req.default_assignee.is_some() {
        sqlx::query(
            "UPDATE step_definitions SET default_assignee_type = ?1, default_assignee_id = ?2 WHERE id = ?3",
        )
        .bind(&atype)
        .bind(&aid)
        .bind(&sid)
        .execute(&state.db)
        .await?;
    }
    if req.requires_approval || req.approver_role_id.is_some() {
        sqlx::query(
            "UPDATE step_definitions SET requires_approval = ?1, approver_role_id = ?2 WHERE id = ?3",
        )
        .bind(req.requires_approval)
        .bind(&role)
        .bind(&sid)
        .execute(&state.db)
        .await?;
    }

    crate::audit::audit(&state.db, &wid, Some(&user.0.id), "step.updated", "step_definition", &sid,
        serde_json::json!({})).await;
    Ok(Json(serde_json::json!({ "ok": true })))
}

pub async fn archive(
    State(state): State<AppState>,
    user: AuthUser,
    Path((wid, sid)): Path<(String, String)>,
) -> AppResult<Json<serde_json::Value>> {
    let ctx = WsCtx::load(&state.db, &user, &wid).await?;
    ctx.require("workflow.create")?;

    let res = sqlx::query(
        "UPDATE step_definitions SET archived_at = ?1 WHERE id = ?2 AND workspace_id = ?3 AND archived_at IS NULL",
    )
    .bind(util::now())
    .bind(&sid)
    .bind(&wid)
    .execute(&state.db)
    .await?;
    if res.rows_affected() == 0 {
        return Err(AppError::NotFound("Adim bulunamadi".into()));
    }
    crate::audit::audit(&state.db, &wid, Some(&user.0.id), "step.archived", "step_definition", &sid,
        serde_json::json!({})).await;
    Ok(Json(serde_json::json!({ "ok": true })))
}
