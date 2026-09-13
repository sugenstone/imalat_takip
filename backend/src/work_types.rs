use axum::extract::{Path, State};
use axum::{routing, Json, Router};
use serde::{Deserialize, Serialize};

use crate::attributes;
use crate::auth::AuthUser;
use crate::error::{AppError, AppResult};
use crate::state::AppState;
use crate::util;
use crate::ws_ctx::WsCtx;

// ---------------------------------------------------------------------------
// Is tipleri
// ---------------------------------------------------------------------------

#[derive(Serialize, sqlx::FromRow)]
pub struct WorkTypeRow {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub is_active: bool,
    pub attribute_count: i64,
}

#[derive(Deserialize)]
pub struct CreateWorkTypeReq {
    name: String,
    #[serde(default)]
    description: Option<String>,
}

#[derive(Deserialize)]
pub struct UpdateWorkTypeReq {
    name: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    is_active: Option<bool>,
    /// Varsayilan akis sablonu (is kalemi olusurken otomatik atanir)
    #[serde(default)]
    default_workflow_template_id: Option<String>,
}

const WT_SELECT: &str = "SELECT t.id, t.name, t.description, t.is_active = 1 AS is_active,
    (SELECT COUNT(*) FROM work_attribute_definitions d WHERE d.work_type_id = t.id AND d.archived_at IS NULL) AS attribute_count
    FROM work_types t";

async fn load_type(db: &sqlx::SqlitePool, wid: &str, tid: &str) -> AppResult<()> {
    let ok: Option<(String,)> = sqlx::query_as(
        "SELECT id FROM work_types WHERE id = ?1 AND workspace_id = ?2 AND archived_at IS NULL",
    )
    .bind(tid)
    .bind(wid)
    .fetch_optional(db)
    .await?;
    if ok.is_some() {
        Ok(())
    } else {
        Err(AppError::NotFound("Is tipi bulunamadi".into()))
    }
}

pub async fn list_types(
    State(state): State<AppState>,
    user: AuthUser,
    Path(wid): Path<String>,
) -> AppResult<Json<Vec<WorkTypeRow>>> {
    WsCtx::load(&state.db, &user, &wid).await?;
    let rows = sqlx::query_as::<_, WorkTypeRow>(&format!(
        "{WT_SELECT} WHERE t.workspace_id = ?1 AND t.archived_at IS NULL ORDER BY t.name"
    ))
    .bind(&wid)
    .fetch_all(&state.db)
    .await?;
    Ok(Json(rows))
}

pub async fn create_type(
    State(state): State<AppState>,
    user: AuthUser,
    Path(wid): Path<String>,
    Json(req): Json<CreateWorkTypeReq>,
) -> AppResult<(axum::http::StatusCode, Json<WorkTypeRow>)> {
    let ctx = WsCtx::load(&state.db, &user, &wid).await?;
    ctx.require("work_item.create")?;

    let name = req.name.trim().to_string();
    if name.is_empty() || name.len() > 60 {
        return Err(AppError::BadRequest("Is tipi adi 1-60 karakter olmali".into()));
    }
    let exists: Option<(String,)> = sqlx::query_as(
        "SELECT id FROM work_types WHERE workspace_id = ?1 AND name = ?2 AND archived_at IS NULL",
    )
    .bind(&wid)
    .bind(&name)
    .fetch_optional(&state.db)
    .await?;
    if exists.is_some() {
        return Err(AppError::Conflict("Bu isimde is tipi zaten var".into()));
    }

    let id = util::new_id();
    let now = util::now();
    sqlx::query(
        "INSERT INTO work_types (id, workspace_id, name, description, is_active, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, 1, ?5, ?5)",
    )
    .bind(&id)
    .bind(&wid)
    .bind(&name)
    .bind(&req.description)
    .bind(&now)
    .execute(&state.db)
    .await?;

    let row = sqlx::query_as::<_, WorkTypeRow>(&format!("{WT_SELECT} WHERE t.id = ?1"))
        .bind(&id)
        .fetch_one(&state.db)
        .await?;
    Ok((axum::http::StatusCode::CREATED, Json(row)))
}

pub async fn update_type(
    State(state): State<AppState>,
    user: AuthUser,
    Path((wid, tid)): Path<(String, String)>,
    Json(req): Json<UpdateWorkTypeReq>,
) -> AppResult<Json<WorkTypeRow>> {
    let ctx = WsCtx::load(&state.db, &user, &wid).await?;
    ctx.require("work_item.create")?;
    load_type(&state.db, &wid, &tid).await?;

    let name = req.name.trim().to_string();
    if name.is_empty() || name.len() > 60 {
        return Err(AppError::BadRequest("Is tipi adi 1-60 karakter olmali".into()));
    }
    // Varsayilan akis dogrulamasi: ayni workspace'in YAYINLANMIS akisi olmali
    if let Some(tfid) = &req.default_workflow_template_id {
        let ok: Option<(String,)> = sqlx::query_as(
            "SELECT t.id FROM workflow_templates t
             WHERE t.id = ?1 AND t.workspace_id = ?2 AND t.archived_at IS NULL
               AND EXISTS (SELECT 1 FROM workflow_versions v WHERE v.template_id = t.id AND v.status = 'published')",
        )
        .bind(tfid)
        .bind(&wid)
        .fetch_optional(&state.db)
        .await?;
        if ok.is_none() {
            return Err(AppError::BadRequest("Gecersiz ya da yayinlanmamis akis".into()));
        }
    }

    sqlx::query(
        "UPDATE work_types SET name = ?1, description = ?2, is_active = ?3, default_workflow_template_id = ?4, updated_at = ?5 WHERE id = ?6",
    )
    .bind(&name)
    .bind(&req.description)
    .bind(req.is_active.unwrap_or(true))
    .bind(&req.default_workflow_template_id)
    .bind(util::now())
    .bind(&tid)
    .execute(&state.db)
    .await?;

    let row = sqlx::query_as::<_, WorkTypeRow>(&format!("{WT_SELECT} WHERE t.id = ?1"))
        .bind(&tid)
        .fetch_one(&state.db)
        .await?;
    Ok(Json(row))
}

pub async fn archive_type(
    State(state): State<AppState>,
    user: AuthUser,
    Path((wid, tid)): Path<(String, String)>,
) -> AppResult<Json<serde_json::Value>> {
    let ctx = WsCtx::load(&state.db, &user, &wid).await?;
    ctx.require("work_item.create")?;
    load_type(&state.db, &wid, &tid).await?;

    let now = util::now();
    sqlx::query("UPDATE work_types SET archived_at = ?1, updated_at = ?1 WHERE id = ?2")
        .bind(&now)
        .bind(&tid)
        .execute(&state.db)
        .await?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

// ---------------------------------------------------------------------------
// Ozellik tanimlari
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct CreateAttributeReq {
    name: String,
    data_type: String,
    #[serde(default)]
    unit: Option<String>,
    #[serde(default)]
    is_required: bool,
    #[serde(default)]
    default_value: Option<String>,
    #[serde(default)]
    is_filterable: bool,
    /// select/multiselect secenekleri (label girilirse value uretilir)
    #[serde(default)]
    options: Vec<String>,
}

#[derive(Deserialize)]
pub struct UpdateAttributeReq {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    data_type: Option<String>,
    #[serde(default)]
    unit: Option<String>,
    #[serde(default)]
    is_required: Option<bool>,
    #[serde(default)]
    default_value: Option<String>,
    #[serde(default)]
    is_filterable: Option<bool>,
    #[serde(default)]
    sort_order: Option<i64>,
    #[serde(default)]
    options: Option<Vec<String>>,
}

/// key'i tip icinde benzersiz uret: slugify + -2, -3...
async fn unique_key(db: &sqlx::SqlitePool, tid: &str, name: &str) -> AppResult<String> {
    let base = util::slugify(name);
    let mut key = base.clone();
    let mut n = 1;
    loop {
        let exists: Option<(String,)> = sqlx::query_as(
            "SELECT id FROM work_attribute_definitions WHERE work_type_id = ?1 AND key = ?2",
        )
        .bind(tid)
        .bind(&key)
        .fetch_optional(db)
        .await?;
        if exists.is_none() {
            return Ok(key);
        }
        n += 1;
        key = format!("{base}-{n}");
    }
}

pub async fn list_attributes(
    State(state): State<AppState>,
    user: AuthUser,
    Path((wid, tid)): Path<(String, String)>,
) -> AppResult<Json<Vec<attributes::DefinitionWithOptions>>> {
    WsCtx::load(&state.db, &user, &wid).await?;
    load_type(&state.db, &wid, &tid).await?;
    let defs = attributes::load_definitions_with_options(&state.db, &tid).await?;
    Ok(Json(defs))
}

pub async fn create_attribute(
    State(state): State<AppState>,
    user: AuthUser,
    Path((wid, tid)): Path<(String, String)>,
    Json(req): Json<CreateAttributeReq>,
) -> AppResult<(axum::http::StatusCode, Json<serde_json::Value>)> {
    let ctx = WsCtx::load(&state.db, &user, &wid).await?;
    ctx.require("work_item.create")?;
    load_type(&state.db, &wid, &tid).await?;

    let name = req.name.trim().to_string();
    if name.is_empty() || name.len() > 60 {
        return Err(AppError::BadRequest("Ozellik adi 1-60 karakter olmali".into()));
    }
    if !attributes::is_valid_data_type(&req.data_type) {
        return Err(AppError::BadRequest(format!(
            "Gecersiz veri tipi: {} (gecerli: {})",
            req.data_type,
            attributes::DATA_TYPES.join(", ")
        )));
    }
    if matches!(req.data_type.as_str(), "select" | "multiselect") && req.options.is_empty() {
        return Err(AppError::BadRequest(
            "select/multiselect icin en az bir secenek gerekli".into(),
        ));
    }

    // default_value tipine uygun mu?
    if let Some(dv) = &req.default_value {
        let v: serde_json::Value = serde_json::from_str(dv)
            .unwrap_or(serde_json::Value::String(dv.clone()));
        attributes::validate_value(&req.data_type, &v, Some(&req.options))?;
    }

    let id = util::new_id();
    let key = unique_key(&state.db, &tid, &name).await?;
    let now = util::now();

    let mut tx = state.db.begin().await?;
    sqlx::query(
        "INSERT INTO work_attribute_definitions
         (id, workspace_id, work_type_id, name, key, data_type, unit, is_required, default_value, sort_order, is_filterable, is_active, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, 1, ?12, ?12)",
    )
    .bind(&id)
    .bind(&wid)
    .bind(&tid)
    .bind(&name)
    .bind(&key)
    .bind(&req.data_type)
    .bind(&req.unit)
    .bind(req.is_required)
    .bind(&req.default_value)
    .bind(0)
    .bind(req.is_filterable)
    .bind(&now)
    .execute(&mut *tx)
    .await?;

    for (i, label) in req.options.iter().enumerate() {
        let value = label.trim().to_string();
        sqlx::query(
            "INSERT INTO work_attribute_options (id, attribute_definition_id, label, value, sort_order, is_active)
             VALUES (?1, ?2, ?3, ?4, ?5, 1)",
        )
        .bind(util::new_id())
        .bind(&id)
        .bind(label.trim())
        .bind(&value)
        .bind(i as i64)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;

    Ok((
        axum::http::StatusCode::CREATED,
        Json(serde_json::json!({ "id": id, "key": key })),
    ))
}

/// Kritik kural (yol haritasi 18): uzerinde GERCEK VERI olan ozelligin
/// veri tipi degistirilemez. Yeni ozellik ac, eskisini pasiflestir.
pub async fn update_attribute(
    State(state): State<AppState>,
    user: AuthUser,
    Path((wid, tid, aid)): Path<(String, String, String)>,
    Json(req): Json<UpdateAttributeReq>,
) -> AppResult<Json<serde_json::Value>> {
    let ctx = WsCtx::load(&state.db, &user, &wid).await?;
    ctx.require("work_item.create")?;
    load_type(&state.db, &wid, &tid).await?;

    let current: Option<(String, i64)> = sqlx::query_as(
        "SELECT data_type, is_active FROM work_attribute_definitions WHERE id = ?1 AND work_type_id = ?2 AND archived_at IS NULL",
    )
    .bind(&aid)
    .bind(&tid)
    .fetch_optional(&state.db)
    .await?;
    let (old_type, _old_active) =
        current.ok_or_else(|| AppError::NotFound("Ozellik bulunamadi".into()))?;

    // Tip degisim korumasi
    if let Some(new_type) = &req.data_type {
        if *new_type != old_type {
            let has_data: Option<(i64,)> = sqlx::query_as(
                "SELECT COUNT(*) FROM work_attribute_values WHERE attribute_definition_id = ?1",
            )
            .bind(&aid)
            .fetch_optional(&state.db)
            .await?;
            let count = has_data.map(|(c,)| c).unwrap_or(0);
            if count > 0 {
                return Err(AppError::Conflict(format!(
                    "Bu ozellikte {count} kayitli deger var; veri tipi degistirilemez. Yeni ozellik olusturup eskisini pasiflestirin."
                )));
            }
            if !attributes::is_valid_data_type(new_type) {
                return Err(AppError::BadRequest(format!("Gecersiz veri tipi: {new_type}")));
            }
        }
    }

    // Secenek guncellemesi: silinen secenekte veri var mi kontrolu
    if let Some(new_options) = &req.options {
        let old_options = attributes::load_options(&state.db, &aid).await?;
        let removed: Vec<&String> = old_options.iter().filter(|o| !new_options.contains(o)).collect();
        if !removed.is_empty() {
            let placeholders = removed.iter().map(|_| "?").collect::<Vec<_>>().join(",");
            let sql = format!(
                "SELECT COUNT(*) FROM work_attribute_values WHERE attribute_definition_id = ? AND value_json LIKE '%' || ? || '%' AND value_text IN ({placeholders})"
            );
            let mut q = sqlx::query_as::<_, (i64,)>(sql.as_str());
            q = q.bind(&aid).bind("[]");
            for r in &removed {
                q = q.bind(r);
            }
            let used: i64 = q
                .fetch_optional(&state.db)
                .await?
                .map(|(c,)| c)
                .unwrap_or(0);
            if used > 0 {
                return Err(AppError::Conflict(
                    "Silinen seceneklerden biri kayitli verilerde kullaniliyor".into(),
                ));
            }
        }
    }

    if let Some(dv) = &req.default_value {
        let effective_type = req.data_type.clone().unwrap_or_else(|| old_type.clone());
        let v: serde_json::Value = serde_json::from_str(dv)
            .unwrap_or(serde_json::Value::String(dv.clone()));
        let opts = match (&req.options, effective_type.as_str()) {
            (Some(o), "select") | (Some(o), "multiselect") => Some(o.clone()),
            _ => None,
        };
        attributes::validate_value(&effective_type, &v, opts.as_ref())?;
    }

    let now = util::now();
    let mut tx = state.db.begin().await?;
    if let Some(n) = &req.name {
        let n = n.trim();
        if n.is_empty() || n.len() > 60 {
            return Err(AppError::BadRequest("Ozellik adi 1-60 karakter olmali".into()));
        }
        sqlx::query("UPDATE work_attribute_definitions SET name = ?1 WHERE id = ?2")
            .bind(n)
            .bind(&aid)
            .execute(&mut *tx)
            .await?;
    }
    if let Some(dt) = &req.data_type {
        sqlx::query("UPDATE work_attribute_definitions SET data_type = ?1 WHERE id = ?2")
            .bind(dt)
            .bind(&aid)
            .execute(&mut *tx)
            .await?;
    }
    if let Some(u) = &req.unit {
        sqlx::query("UPDATE work_attribute_definitions SET unit = ?1 WHERE id = ?2")
            .bind(u)
            .bind(&aid)
            .execute(&mut *tx)
            .await?;
    }
    if let Some(r) = req.is_required {
        sqlx::query("UPDATE work_attribute_definitions SET is_required = ?1 WHERE id = ?2")
            .bind(r)
            .bind(&aid)
            .execute(&mut *tx)
            .await?;
    }
    if let Some(d) = &req.default_value {
        sqlx::query("UPDATE work_attribute_definitions SET default_value = ?1 WHERE id = ?2")
            .bind(d)
            .bind(&aid)
            .execute(&mut *tx)
            .await?;
    }
    if let Some(f) = req.is_filterable {
        sqlx::query("UPDATE work_attribute_definitions SET is_filterable = ?1 WHERE id = ?2")
            .bind(f)
            .bind(&aid)
            .execute(&mut *tx)
            .await?;
    }
    if let Some(s) = req.sort_order {
        sqlx::query("UPDATE work_attribute_definitions SET sort_order = ?1 WHERE id = ?2")
            .bind(s)
            .bind(&aid)
            .execute(&mut *tx)
            .await?;
    }
    if let Some(options) = &req.options {
        sqlx::query("DELETE FROM work_attribute_options WHERE attribute_definition_id = ?1")
            .bind(&aid)
            .execute(&mut *tx)
            .await?;
        for (i, label) in options.iter().enumerate() {
            sqlx::query(
                "INSERT INTO work_attribute_options (id, attribute_definition_id, label, value, sort_order, is_active)
                 VALUES (?1, ?2, ?3, ?4, ?5, 1)",
            )
            .bind(util::new_id())
            .bind(&aid)
            .bind(label.trim())
            .bind(label.trim())
            .bind(i as i64)
            .execute(&mut *tx)
            .await?;
        }
    }
    sqlx::query("UPDATE work_attribute_definitions SET updated_at = ?1 WHERE id = ?2")
        .bind(&now)
        .bind(&aid)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;

    Ok(Json(serde_json::json!({ "ok": true })))
}

pub async fn archive_attribute(
    State(state): State<AppState>,
    user: AuthUser,
    Path((wid, tid, aid)): Path<(String, String, String)>,
) -> AppResult<Json<serde_json::Value>> {
    let ctx = WsCtx::load(&state.db, &user, &wid).await?;
    ctx.require("work_item.create")?;
    load_type(&state.db, &wid, &tid).await?;

    let now = util::now();
    let res = sqlx::query(
        "UPDATE work_attribute_definitions SET archived_at = ?1, updated_at = ?1, is_active = 0 WHERE id = ?2 AND work_type_id = ?3 AND archived_at IS NULL",
    )
    .bind(&now)
    .bind(&aid)
    .bind(&tid)
    .execute(&state.db)
    .await?;
    if res.rows_affected() == 0 {
        return Err(AppError::NotFound("Ozellik bulunamadi".into()));
    }
    Ok(Json(serde_json::json!({ "ok": true })))
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/workspaces/{wid}/work-types",
            routing::get(list_types).post(create_type),
        )
        .route(
            "/workspaces/{wid}/work-types/{tid}",
            routing::patch(update_type),
        )
        .route(
            "/workspaces/{wid}/work-types/{tid}/archive",
            routing::post(archive_type),
        )
        .route(
            "/workspaces/{wid}/work-types/{tid}/attributes",
            routing::get(list_attributes).post(create_attribute),
        )
        .route(
            "/workspaces/{wid}/work-types/{tid}/attributes/{aid}",
            routing::patch(update_attribute),
        )
        .route(
            "/workspaces/{wid}/work-types/{tid}/attributes/{aid}/archive",
            routing::post(archive_attribute),
        )
}
