//! Faz 8: Dosya/fotograf ekleri (yol haritasi 34).
//! Depolama: data/uploads/{wid}/{aid}_{dosya} — kayit DB'de, silme soft.

use axum::extract::{Multipart, Path, Query, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::{routing, Json, Router};
use serde::{Deserialize, Serialize};

use crate::auth::AuthUser;
use crate::comments::check_entity_scope;
use crate::error::{AppError, AppResult};
use crate::state::AppState;
use crate::util;
use crate::ws_ctx::WsCtx;

const MAX_SIZE: usize = 10 * 1024 * 1024; // 10MB

#[derive(Serialize, sqlx::FromRow)]
pub struct AttachmentRow {
    pub id: String,
    pub entity_type: String,
    pub entity_id: String,
    pub file_name: String,
    pub mime_type: String,
    pub size: i64,
    pub created_at: String,
    pub uploader_name: String,
}

#[derive(Deserialize)]
pub struct ListQuery {
    entity_type: String,
    entity_id: String,
}

/// Dosya adini sanitize et: yol ayracolari ve .. kaldir.
fn sanitize_name(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '-',
            c => c,
        })
        .collect();
    let trimmed = cleaned.trim().trim_start_matches('.').to_string();
    if trimmed.is_empty() {
        "dosya".to_string()
    } else {
        trimmed.chars().take(120).collect()
    }
}

fn upload_root() -> std::path::PathBuf {
    std::env::var("IMTK_UPLOAD_DIR").unwrap_or_else(|_| "data/uploads".into()).into()
}

/// Dosya yukle (multipart: entity_type, entity_id, file).
pub async fn upload(
    State(state): State<AppState>,
    user: AuthUser,
    Path(wid): Path<String>,
    mut multipart: Multipart,
) -> AppResult<(StatusCode, Json<AttachmentRow>)> {
    let ctx = WsCtx::load(&state.db, &user, &wid).await?;

    let mut entity_type = String::new();
    let mut entity_id = String::new();
    let mut file_name: Option<String> = None;
    let mut mime = "application/octet-stream".to_string();
    let mut bytes: Option<Vec<u8>> = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| AppError::BadRequest(format!("Yukleme okunamadi: {e}")))?
    {
        match field.name().unwrap_or("") {
            "entity_type" => entity_type = field.text().await.unwrap_or_default(),
            "entity_id" => entity_id = field.text().await.unwrap_or_default(),
            "file" => {
                if let Some(fn_) = field.file_name() {
                    file_name = Some(sanitize_name(fn_));
                }
                if let Some(ct) = field.content_type() {
                    mime = ct.to_string();
                }
                let data = field
                    .bytes()
                    .await
                    .map_err(|e| AppError::BadRequest(format!("Dosya okunamadi: {e}")))?;
                bytes = Some(data.to_vec());
            }
            _ => {}
        }
    }

    let bytes = bytes.ok_or_else(|| AppError::BadRequest("Dosya eksik".into()))?;
    let file_name = file_name.unwrap_or_else(|| "dosya".into());
    if entity_type.is_empty() || entity_id.is_empty() {
        return Err(AppError::BadRequest("entity_type ve entity_id zorunlu".into()));
    }
    if bytes.is_empty() {
        return Err(AppError::BadRequest("Bos dosya".into()));
    }
    if bytes.len() > MAX_SIZE {
        return Err(AppError::BadRequest("Dosya en fazla 10MB olabilir".into()));
    }
    if matches!(mime.as_str(), "application/x-msdownload" | "application/x-dos-batch")
        || file_name.ends_with(".exe") || file_name.ends_with(".bat") || file_name.ends_with(".cmd")
    {
        return Err(AppError::BadRequest("Bu dosya turu reddedildi".into()));
    }

    check_entity_scope(&state.db, &ctx, &wid, &entity_type, &entity_id).await?;

    let id = util::new_id();
    let now = util::now();
    let rel_key = format!("{wid}/{}_{file_name}", id);
    let disk_path = upload_root().join(&rel_key);
    if let Some(parent) = disk_path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| AppError::BadRequest(format!("Klasor olusturulamadi: {e}")))?;
    }
    std::fs::write(&disk_path, &bytes)
        .map_err(|e| AppError::BadRequest(format!("Dosya yazilamadi: {e}")))?;

    sqlx::query(
        "INSERT INTO attachments (id, workspace_id, entity_type, entity_id, storage_key, file_name, mime_type, size, uploaded_by, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
    )
    .bind(&id)
    .bind(&wid)
    .bind(&entity_type)
    .bind(&entity_id)
    .bind(&rel_key)
    .bind(&file_name)
    .bind(&mime)
    .bind(bytes.len() as i64)
    .bind(&user.0.id)
    .bind(&now)
    .execute(&state.db)
    .await?;

    let row = sqlx::query_as::<_, AttachmentRow>(
        "SELECT a.id, a.entity_type, a.entity_id, a.file_name, a.mime_type, a.size, a.created_at,
                (SELECT u.name FROM users u WHERE u.id = a.uploaded_by) AS uploader_name
         FROM attachments a WHERE a.id = ?1",
    )
    .bind(&id)
    .fetch_one(&state.db)
    .await?;
    Ok((StatusCode::CREATED, Json(row)))
}

pub async fn list(
    State(state): State<AppState>,
    user: AuthUser,
    Path(wid): Path<String>,
    Query(q): Query<ListQuery>,
) -> AppResult<Json<Vec<AttachmentRow>>> {
    let ctx = WsCtx::load(&state.db, &user, &wid).await?;
    check_entity_scope(&state.db, &ctx, &wid, &q.entity_type, &q.entity_id).await?;

    let rows = sqlx::query_as::<_, AttachmentRow>(
        "SELECT a.id, a.entity_type, a.entity_id, a.file_name, a.mime_type, a.size, a.created_at,
                (SELECT u.name FROM users u WHERE u.id = a.uploaded_by) AS uploader_name
         FROM attachments a
         WHERE a.workspace_id = ?1 AND a.entity_type = ?2 AND a.entity_id = ?3 AND a.archived_at IS NULL
         ORDER BY a.created_at DESC",
    )
    .bind(&wid)
    .bind(&q.entity_type)
    .bind(&q.entity_id)
    .fetch_all(&state.db)
    .await?;
    Ok(Json(rows))
}

/// Dosyayi indir.
pub async fn download(
    State(state): State<AppState>,
    user: AuthUser,
    Path((wid, aid)): Path<(String, String)>,
) -> AppResult<Response> {
    let ctx = WsCtx::load(&state.db, &user, &wid).await?;
    let row: Option<(String, String, String, String)> = sqlx::query_as(
        "SELECT storage_key, file_name, mime_type, entity_id FROM attachments
         WHERE id = ?1 AND workspace_id = ?2 AND archived_at IS NULL",
    )
    .bind(&aid)
    .bind(&wid)
    .fetch_optional(&state.db)
    .await?;
    let (key, file_name, mime, entity_id) =
        row.ok_or_else(|| AppError::NotFound("Dosya bulunamadi".into()))?;

    // Yetki: kayitli varligin kapsaminda olmali
    let et: Option<(String,)> = sqlx::query_as(
        "SELECT entity_type FROM attachments WHERE id = ?1",
    )
    .bind(&aid)
    .fetch_optional(&state.db)
    .await?;
    if let Some((etype,)) = et {
        check_entity_scope(&state.db, &ctx, &wid, &etype.as_str(), &entity_id).await?;
    }

    let disk = upload_root().join(&key);
    let bytes = std::fs::read(&disk).map_err(|_| AppError::NotFound("Dosya diskte yok".into()))?;

    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        mime.parse().unwrap_or(header::HeaderValue::from_static("application/octet-stream")),
    );
    headers.insert(
        header::CONTENT_DISPOSITION,
        format!("attachment; filename=\"{}\"", file_name.replace('"', ""))
            .parse()
            .unwrap_or(header::HeaderValue::from_static("attachment")),
    );
    Ok((headers, bytes).into_response())
}

/// Dosyayi arsivle (soft delete — diskte kalir).
pub async fn archive(
    State(state): State<AppState>,
    user: AuthUser,
    Path((wid, aid)): Path<(String, String)>,
) -> AppResult<Json<serde_json::Value>> {
    let _ = user;
    let _ctx = WsCtx::load(&state.db, &user, &wid).await?;
    let res = sqlx::query(
        "UPDATE attachments SET archived_at = ?1 WHERE id = ?2 AND workspace_id = ?3 AND archived_at IS NULL",
    )
    .bind(util::now())
    .bind(&aid)
    .bind(&wid)
    .execute(&state.db)
    .await?;
    if res.rows_affected() == 0 {
        return Err(AppError::NotFound("Dosya bulunamadi".into()));
    }
    Ok(Json(serde_json::json!({ "ok": true })))
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/workspaces/{wid}/attachments",
            routing::get(list).post(upload),
        )
        .route(
            "/workspaces/{wid}/attachments/{aid}/file",
            routing::get(download),
        )
        .route("/workspaces/{wid}/attachments/{aid}", routing::delete(archive))
}
