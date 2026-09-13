//! Faz 8: Yorum sistemi (yol haritasi 33) — polymorphic:
//! section | work_item | process_instance uzerine yorum.

use axum::extract::{Path, Query, State};
use axum::{routing, Json, Router};
use serde::{Deserialize, Serialize};

use crate::auth::AuthUser;
use crate::error::{AppError, AppResult};
use crate::state::AppState;
use crate::util;
use crate::ws_ctx::WsCtx;

#[derive(Serialize, sqlx::FromRow)]
pub struct CommentRow {
    pub id: String,
    pub entity_type: String,
    pub entity_id: String,
    pub body: String,
    pub created_by: String,
    pub created_at: String,
    pub edited_at: Option<String>,
    pub author_name: String,
}

#[derive(Deserialize)]
pub struct ListQuery {
    entity_type: String,
    entity_id: String,
}

#[derive(Deserialize)]
pub struct CreateCommentReq {
    entity_type: String,
    entity_id: String,
    body: String,
}

#[derive(Deserialize)]
pub struct UpdateCommentReq {
    body: String,
}

/// Varlik bu workspace'e ait mi + kapsam kontrolu.
/// process_instance icin bagli work item'in section kapsamina bakilir.
pub async fn check_entity_scope(
    db: &sqlx::SqlitePool,
    ctx: &WsCtx,
    wid: &str,
    entity_type: &str,
    entity_id: &str,
) -> AppResult<()> {
    match entity_type {
        "section" => {
            let row: Option<(String,)> = sqlx::query_as(
                "SELECT path_cache FROM sections WHERE id = ?1 AND workspace_id = ?2 AND archived_at IS NULL",
            )
            .bind(entity_id)
            .bind(wid)
            .fetch_optional(db)
            .await?;
            match row {
                Some((path,)) if ctx.path_in_scope(&path) => Ok(()),
                Some(_) => Err(AppError::Forbidden("Kapsam disi bolum".into())),
                None => Err(AppError::NotFound("Bolum bulunamadi".into())),
            }
        }
        "work_item" => {
            let row: Option<(String,)> = sqlx::query_as(
                "SELECT s.path_cache FROM work_items w JOIN sections s ON s.id = w.section_id
                 WHERE w.id = ?1 AND w.workspace_id = ?2 AND w.archived_at IS NULL",
            )
            .bind(entity_id)
            .bind(wid)
            .fetch_optional(db)
            .await?;
            match row {
                Some((path,)) if ctx.path_in_scope(&path) => Ok(()),
                Some(_) => Err(AppError::Forbidden("Kapsam disi is kalemi".into())),
                None => Err(AppError::NotFound("Is kalemi bulunamadi".into())),
            }
        }
        "process_instance" => {
            let row: Option<(String, String)> = sqlx::query_as(
                "SELECT w.workspace_id, s.path_cache
                 FROM process_instances p
                 JOIN workflow_instances wi ON wi.id = p.workflow_instance_id
                 JOIN work_items w ON w.id = wi.work_item_id
                 JOIN sections s ON s.id = w.section_id
                 WHERE p.id = ?1",
            )
            .bind(entity_id)
            .fetch_optional(db)
            .await?;
            match row {
                Some((ws, path)) if ws == wid && ctx.path_in_scope(&path) => Ok(()),
                Some((ws, _)) if ws == wid => Err(AppError::Forbidden("Kapsam disi surec".into())),
                _ => Err(AppError::NotFound("Surec bulunamadi".into())),
            }
        }
        _ => Err(AppError::BadRequest(
            "entity_type section, work_item veya process_instance olmali".into(),
        )),
    }
}

const SELECT: &str = "SELECT c.id, c.entity_type, c.entity_id, c.body, c.created_by, c.created_at, c.edited_at,
    (SELECT u.name FROM users u WHERE u.id = c.created_by) AS author_name
    FROM comments c";

pub async fn list(
    State(state): State<AppState>,
    user: AuthUser,
    Path(wid): Path<String>,
    Query(q): Query<ListQuery>,
) -> AppResult<Json<Vec<CommentRow>>> {
    let ctx = WsCtx::load(&state.db, &user, &wid).await?;
    check_entity_scope(&state.db, &ctx, &wid, &q.entity_type, &q.entity_id).await?;

    let rows = sqlx::query_as::<_, CommentRow>(&format!(
        "{SELECT} WHERE c.workspace_id = ?1 AND c.entity_type = ?2 AND c.entity_id = ?3
         ORDER BY c.created_at ASC LIMIT 500"
    ))
    .bind(&wid)
    .bind(&q.entity_type)
    .bind(&q.entity_id)
    .fetch_all(&state.db)
    .await?;
    Ok(Json(rows))
}

pub async fn create(
    State(state): State<AppState>,
    user: AuthUser,
    Path(wid): Path<String>,
    Json(req): Json<CreateCommentReq>,
) -> AppResult<(axum::http::StatusCode, Json<CommentRow>)> {
    let ctx = WsCtx::load(&state.db, &user, &wid).await?;
    check_entity_scope(&state.db, &ctx, &wid, &req.entity_type, &req.entity_id).await?;

    let body = req.body.trim().to_string();
    if body.is_empty() || body.len() > 4000 {
        return Err(AppError::BadRequest("Yorum 1-4000 karakter olmali".into()));
    }

    let id = util::new_id();
    let now = util::now();
    sqlx::query(
        "INSERT INTO comments (id, workspace_id, entity_type, entity_id, body, created_by, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
    )
    .bind(&id)
    .bind(&wid)
    .bind(&req.entity_type)
    .bind(&req.entity_id)
    .bind(&body)
    .bind(&user.0.id)
    .bind(&now)
    .execute(&state.db)
    .await?;

    let row = sqlx::query_as::<_, CommentRow>(&format!("{SELECT} WHERE c.id = ?1"))
        .bind(&id)
        .fetch_one(&state.db)
        .await?;
    Ok((axum::http::StatusCode::CREATED, Json(row)))
}

/// Sadece kendi yorumu duzenlenebilir.
pub async fn update(
    State(state): State<AppState>,
    user: AuthUser,
    Path((wid, cid)): Path<(String, String)>,
    Json(req): Json<UpdateCommentReq>,
) -> AppResult<Json<CommentRow>> {
    let ctx = WsCtx::load(&state.db, &user, &wid).await?;
    let _ = ctx;

    let existing: Option<(String,)> = sqlx::query_as(
        "SELECT workspace_id FROM comments WHERE id = ?1",
    )
    .bind(&cid)
    .fetch_optional(&state.db)
    .await?;
    let (ws,) = existing.ok_or_else(|| AppError::NotFound("Yorum bulunamadi".into()))?;
    if ws != wid {
        return Err(AppError::NotFound("Yorum bulunamadi".into()));
    }

    let body = req.body.trim().to_string();
    if body.is_empty() || body.len() > 4000 {
        return Err(AppError::BadRequest("Yorum 1-4000 karakter olmali".into()));
    }
    let res = sqlx::query(
        "UPDATE comments SET body = ?1, edited_at = ?2 WHERE id = ?3 AND created_by = ?4",
    )
    .bind(&body)
    .bind(util::now())
    .bind(&cid)
    .bind(&user.0.id)
    .execute(&state.db)
    .await?;
    if res.rows_affected() == 0 {
        return Err(AppError::Forbidden("Sadece kendi yorumunuzu duzenleyebilirsiniz".into()));
    }

    let row = sqlx::query_as::<_, CommentRow>(&format!("{SELECT} WHERE c.id = ?1"))
        .bind(&cid)
        .fetch_one(&state.db)
        .await?;
    Ok(Json(row))
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/workspaces/{wid}/comments",
            routing::get(list).post(create),
        )
        .route("/workspaces/{wid}/comments/{cid}", routing::patch(update))
}
