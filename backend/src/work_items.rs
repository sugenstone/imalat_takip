use axum::extract::{Path, Query, State};
use axum::{routing, Json, Router};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::attributes::{self, TypedValue};
use crate::auth::AuthUser;
use crate::error::{AppError, AppResult};
use crate::state::AppState;
use crate::util;
use crate::ws_ctx::WsCtx;

pub const PRIORITIES: &[&str] = &["low", "medium", "high", "urgent"];
pub const STATUSES: &[&str] = &["draft", "active", "blocked", "completed", "cancelled"];

#[derive(Serialize, sqlx::FromRow)]
pub struct WorkItemRow {
    pub id: String,
    pub section_id: String,
    pub section_name: String,
    pub work_type_id: Option<String>,
    pub work_type_name: Option<String>,
    pub name: String,
    pub description: Option<String>,
    pub priority: String,
    pub status: String,
    pub planned_start: Option<String>,
    pub planned_end: Option<String>,
    pub created_at: String,
    /// Aktif akisin surec ilerlemesi (kart yuzdesi icin)
    pub process_total: i64,
    pub process_approved: i64,
}

#[derive(Serialize)]
pub struct AttributeValueOut {
    pub definition_id: String,
    pub key: String,
    pub name: String,
    pub data_type: String,
    pub unit: Option<String>,
    pub is_required: bool,
    pub value: serde_json::Value,
    pub options: Vec<String>,
}

#[derive(Serialize)]
pub struct WorkItemDetail {
    #[serde(flatten)]
    pub base: WorkItemRow,
    pub section_path: Vec<String>,
    pub attributes: Vec<AttributeValueOut>,
}

#[derive(Deserialize)]
pub struct CreateItemReq {
    section_id: String,
    /// Tipin varsayilan akisi varsa otomatik ata
    #[serde(default)]
    auto_workflow: bool,
    #[serde(default)]
    work_type_id: Option<String>,
    /// Secili surec grubunu (workflow template) olusturulan kaleme ata
    #[serde(default)]
    pub workflow_template_id: Option<String>,
    name: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    priority: Option<String>,
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    planned_start: Option<String>,
    #[serde(default)]
    planned_end: Option<String>,
    #[serde(default)]
    attributes: Vec<AttributeValueIn>,
}

#[derive(Deserialize, Clone)]
pub struct AttributeValueIn {
    pub definition_id: String,
    pub value: serde_json::Value,
}

#[derive(Deserialize)]
pub struct BulkCreateReq {
    section_id: String,
    #[serde(default)]
    work_type_id: Option<String>,
    template: String,
    start: i64,
    end: i64,
    #[serde(default)]
    priority: Option<String>,
    #[serde(default)]
    status: Option<String>,
}

#[derive(Deserialize)]
pub struct UpdateItemReq {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    section_id: Option<String>,
    #[serde(default)]
    work_type_id: Option<String>,
    #[serde(default)]
    priority: Option<String>,
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    planned_start: Option<String>,
    #[serde(default)]
    planned_end: Option<String>,
}

#[derive(Deserialize)]
pub struct ListQuery {
    #[serde(default)]
    section_id: Option<String>,
    /// true ise section subtree dahil
    #[serde(default)]
    subtree: bool,
    #[serde(default)]
    work_type_id: Option<String>,
    #[serde(default)]
    status: Option<String>,
    /// isim/aciklama arama
    #[serde(default)]
    search: Option<String>,
    #[serde(default)]
    include_archived: bool,
    /// Faz 6: bana atanan is kalemleri (user + takim atamalari)
    #[serde(default)]
    mine: bool,
    /// Faz 6: icinde onay bekleyen (submitted) sureci olan is kalemleri
    #[serde(default)]
    pending_approval: bool,
    /// q_<key>=<deger> seklinde dinamik ozellik filtreleri
    #[serde(flatten)]
    attr_filters: HashMap<String, String>,
}


const ITEM_PROGRESS: &str = ",
    (SELECT COUNT(*) FROM process_instances pi
      JOIN workflow_instances wi ON wi.id = pi.workflow_instance_id
      WHERE wi.work_item_id = w.id AND wi.status = 'active') AS process_total,
    (SELECT COUNT(*) FROM process_instances pi2
      JOIN workflow_instances wi2 ON wi2.id = pi2.workflow_instance_id
      WHERE wi2.work_item_id = w.id AND wi2.status = 'active' AND pi2.status = 'approved') AS process_approved";

fn validate_priority(p: &str) -> AppResult<()> {
    if PRIORITIES.contains(&p) {
        Ok(())
    } else {
        Err(AppError::BadRequest(format!(
            "Gecersiz onem: {p} (gecerli: {})",
            PRIORITIES.join(", ")
        )))
    }
}

fn validate_status(s: &str) -> AppResult<()> {
    if STATUSES.contains(&s) {
        Ok(())
    } else {
        Err(AppError::BadRequest(format!(
            "Gecersiz durum: {s} (gecerli: {})",
            STATUSES.join(", ")
        )))
    }
}

fn validate_date(s: &str) -> AppResult<()> {
    chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d")
        .map(|_| ())
        .map_err(|_| AppError::BadRequest("Tarih formati YYYY-MM-DD olmali".into()))
}

/// Section yukle + kapsam kontrolu. (path_cache dondurur)
async fn load_section_checked(
    db: &sqlx::SqlitePool,
    ctx: &WsCtx,
    wid: &str,
    sid: &str,
) -> AppResult<String> {
    let row: Option<(String, Option<String>, String, i64)> = sqlx::query_as(
        "SELECT id, parent_id, path_cache, depth FROM sections WHERE id = ?1 AND workspace_id = ?2 AND archived_at IS NULL",
    )
    .bind(sid)
    .bind(wid)
    .fetch_optional(db)
    .await?;
    let (_, _, path, _) =
        row.ok_or_else(|| AppError::NotFound("Bolum bulunamadi".into()))?;
    if !ctx.path_in_scope(&path) {
        return Err(AppError::Forbidden("Kapsam disi bolum".into()));
    }
    Ok(path)
}

/// Tanim dogrulama + deger yazimi. Tek transaction icinde cagirilir.
async fn upsert_attribute_values(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    work_item_id: &str,
    defs: &HashMap<String, attributes::DefinitionWithOptions>,
    values_in: &[AttributeValueIn],
    user_id: &str,
    require_all: bool,
) -> AppResult<()> {
    let now = util::now();

    if require_all {
        // Zorunlu alan kontrolu (yol haritasi 17: is_required)
        let provided: std::collections::HashSet<&str> =
            values_in.iter().map(|v| v.definition_id.as_str()).collect();
        for d in defs.values() {
            if d.is_required && !provided.contains(d.id.as_str()) {
                return Err(AppError::BadRequest(format!(
                    "Zorunlu ozellik eksik: {}",
                    d.name
                )));
            }
        }
    }

    for v in values_in {
        let def = defs.get(&v.definition_id).ok_or_else(|| {
            AppError::BadRequest("Ozellik tanimi bu is tipine ait degil".into())
        })?;
        let typed = attributes::validate_value(&def.data_type, &v.value, Some(&def.options))?;

        // Diger tip kolonlarini sifirla (tip degismis olabilir)
        sqlx::query(
            "INSERT INTO work_attribute_values
             (id, work_item_id, attribute_definition_id, value_text, value_number, value_boolean, value_date, value_datetime, value_json, updated_by, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
             ON CONFLICT(work_item_id, attribute_definition_id) DO UPDATE SET
                value_text = NULL, value_number = NULL, value_boolean = NULL,
                value_date = NULL, value_datetime = NULL, value_json = NULL,
                updated_by = excluded.updated_by, updated_at = excluded.updated_at",
        )
        .bind(util::new_id())
        .bind(work_item_id)
        .bind(&def.id)
        .bind(Option::<String>::None)
        .bind(Option::<f64>::None)
        .bind(Option::<i64>::None)
        .bind(Option::<String>::None)
        .bind(Option::<String>::None)
        .bind(Option::<String>::None)
        .bind(user_id)
        .bind(&now)
        .execute(&mut **tx)
        .await?;

        // Uygun kolona yaz
        let (col_expr, val): (&'static str, String) = match &typed {
            TypedValue::Text(s) => ("value_text", s.clone()),
            TypedValue::Number(n) => ("value_number", n.to_string()),
            TypedValue::Boolean(b) => ("value_boolean", if *b { "1".into() } else { "0".into() }),
            TypedValue::Date(s) => ("value_date", s.clone()),
            TypedValue::Datetime(s) => ("value_datetime", s.clone()),
            TypedValue::Json(s) => ("value_json", s.clone()),
        };
        let sql = format!(
            "UPDATE work_attribute_values SET {col_expr} = ?1
             WHERE work_item_id = ?2 AND attribute_definition_id = ?3"
        );
        sqlx::query(&sql)
            .bind(&val)
            .bind(work_item_id)
            .bind(&def.id)
            .execute(&mut **tx)
            .await?;
    }
    Ok(())
}

/// Tanimlari map olarak yukle.
async fn defs_for_type(
    db: &sqlx::SqlitePool,
    work_type_id: &str,
) -> AppResult<HashMap<String, attributes::DefinitionWithOptions>> {
    let defs = attributes::load_definitions_with_options(db, work_type_id).await?;
    Ok(defs.into_iter().map(|d| (d.id.clone(), d)).collect())
}

// ---------------------------------------------------------------------------
// Liste
// ---------------------------------------------------------------------------

pub async fn list(
    State(state): State<AppState>,
    user: AuthUser,
    Path(wid): Path<String>,
    Query(q): Query<ListQuery>,
) -> AppResult<Json<Vec<WorkItemRow>>> {
    let ctx = WsCtx::load(&state.db, &user, &wid).await?;

    // Dinamik ozellik filtrelerini ayir (q_ oneki)
    let mut attr_filters: Vec<(String, String)> = Vec::new();
    for (k, v) in &q.attr_filters {
        if let Some(key) = k.strip_prefix("q_") {
            if !key.is_empty() {
                attr_filters.push((key.to_string(), v.clone()));
            }
        }
    }

    // SQL kosullari
    let mut conditions: Vec<String> = vec!["w.workspace_id = ?1".to_string()];
    let mut params: Vec<String> = vec![wid.clone()];
    let mut idx = 1;

    if !q.include_archived {
        conditions.push("w.archived_at IS NULL".into());
    }
    if let Some(s) = &q.status {
        idx += 1;
        conditions.push(format!("w.status = ?{idx}"));
        params.push(s.clone());
    }
    if let Some(t) = &q.work_type_id {
        idx += 1;
        conditions.push(format!("w.work_type_id = ?{idx}"));
        params.push(t.clone());
    }
    if let Some(s) = &q.search {
        idx += 1;
        conditions.push(format!("(w.name LIKE '%' || ?{idx} || '%' OR w.description LIKE '%' || ?{idx} || '%')"));
        params.push(s.clone());
    }

    // Bolum filtresi (subtree destegi)
    let section_paths: Vec<String> = if let Some(sid) = &q.section_id {
        let path = load_section_checked(&state.db, &ctx, &wid, sid).await?;
        if q.subtree {
            vec![path]
        } else {
            // Tek bolum: path esitligi
            idx += 1;
            conditions.push(format!(
                "w.section_id IN (SELECT id FROM sections WHERE path_cache = ?{idx})"
            ));
            params.push(path);
            Vec::new()
        }
    } else {
        Vec::new()
    };
    if !section_paths.is_empty() {
        // subtree: path_cache LIKE 'path/%' OR path_cache = 'path'
        // Her path 2 placeholder uretir (LIKE prefix + esitlik), 2 bind edilir.
        let ors: Vec<String> = section_paths
            .iter()
            .map(|_p| {
                idx += 1;
                let i1 = idx;
                idx += 1;
                format!(
                    "w.section_id IN (SELECT id FROM sections WHERE path_cache LIKE ?{i1} || '/%' OR path_cache = ?{idx})"
                )
            })
            .collect();
        for p in &section_paths {
            params.push(p.clone());
            params.push(p.clone());
        }
        conditions.push(format!("({})", ors.join(" OR ")));
    }

    // Kapsam filtresi: kapsamli kullanici sadece subtree'lerindeki bolumleri gorur
    if let Some(scopes) = &ctx.scope_paths {
        if !scopes.is_empty() {
            let ors: Vec<String> = scopes
                .iter()
                .map(|_p| {
                    idx += 1;
                    let i1 = idx;
                    idx += 1;
                    format!(
                        "w.section_id IN (SELECT id FROM sections WHERE path_cache = ?{i1} OR path_cache LIKE ?{idx} || '/%')"
                    )
                })
                .collect();
            for p in scopes {
                params.push(p.clone());
                params.push(p.clone());
            }
            conditions.push(format!("({})", ors.join(" OR ")));
        }
    }

    // Faz 6: Bana atananlar — user direkt + uyesi olunan takim atamalari
    if q.mine {
        idx += 1;
        let ws_p = idx;
        idx += 1;
        let user_p = idx;
        conditions.push(format!(
            "w.id IN (SELECT wi2.work_item_id FROM workflow_instances wi2
                       JOIN work_items w2 ON w2.id = wi2.work_item_id
                       JOIN process_instances pi ON pi.workflow_instance_id = wi2.id
                       JOIN process_assignments pa ON pa.process_instance_id = pi.id
                       WHERE w2.workspace_id = ?{ws_p} AND pa.unassigned_at IS NULL
                         AND ((pa.assignee_type = 'user' AND pa.assignee_id = ?{user_p})
                           OR (pa.assignee_type = 'team' AND pa.assignee_id IN
                               (SELECT team_id FROM team_members WHERE user_id = ?{user_p}))))"
        ));
        params.push(wid.clone());
        params.push(user.0.id.clone());
    }

    // Faz 6: Icinde onay bekleyen (submitted) sureci olan is kalemleri
    if q.pending_approval {
        conditions.push(
            "w.id IN (SELECT wi3.work_item_id FROM workflow_instances wi3
                       JOIN process_instances pi3 ON pi3.workflow_instance_id = wi3.id
                       WHERE pi3.status = 'submitted' AND wi3.status = 'active')"
                .to_string(),
        );
    }

    // Dinamik ozellik filtreleri
    if !attr_filters.is_empty() {
        for (key, val) in &attr_filters {
            let def: Option<(String, String)> = sqlx::query_as(
                "SELECT id, data_type FROM work_attribute_definitions
                 WHERE workspace_id = ?1 AND key = ?2 AND archived_at IS NULL",
            )
            .bind(&wid)
            .bind(key)
            .fetch_optional(&state.db)
            .await?;
            let (def_id, data_type) =
                def.ok_or_else(|| AppError::BadRequest(format!("Bilinmeyen ozellik anahtari: {key}")))?;

            let col = match data_type.as_str() {
                "integer" | "decimal" | "currency" | "percentage" => "value_number",
                "boolean" => "value_boolean",
                "date" => "value_date",
                "datetime" => "value_datetime",
                "multiselect" => "value_json",
                _ => "value_text",
            };
            let op = if data_type == "multiselect" {
                // dizi icerir
                idx += 1;
                let i = idx;
                params.push(format!("%\"{val}\"%"));
                format!("av.{col} LIKE ?{i}")
            } else {
                idx += 1;
                let i = idx;
                let bind_val = match data_type.as_str() {
                    "integer" | "decimal" | "currency" | "percentage" => {
                        val.replace(',', ".").parse::<f64>()
                            .map_err(|_| AppError::BadRequest("Ozellik filtresi sayi degil".into()))?
                            .to_string()
                    }
                    "boolean" => match val.as_str() {
                        "true" | "1" => "1".to_string(),
                        "false" | "0" => "0".to_string(),
                        _ => return Err(AppError::BadRequest("Boolean filtre true/false olmali".into())),
                    },
                    _ => val.clone(),
                };
                params.push(bind_val);
                format!("av.{col} = ?{i}")
            };
            conditions.push(format!(
                "w.id IN (SELECT work_item_id FROM work_attribute_values av WHERE av.attribute_definition_id = '{def_id}' AND {op})"
            ));
        }
    }

    let sql = format!(
        "SELECT w.id, w.section_id, s.name AS section_name, w.work_type_id, t.name AS work_type_name,
                w.name, w.description, w.priority, w.status, w.planned_start, w.planned_end, w.created_at{ITEM_PROGRESS}
         FROM work_items w
         JOIN sections s ON s.id = w.section_id
         LEFT JOIN work_types t ON t.id = w.work_type_id
         WHERE {}
         ORDER BY w.created_at DESC
         LIMIT 500",
        conditions.join(" AND ")
    );

    let mut query = sqlx::query_as::<_, WorkItemRow>(&sql);
    for p in &params {
        query = query.bind(p);
    }
    let rows = query.fetch_all(&state.db).await?;
    Ok(Json(rows))
}

// ---------------------------------------------------------------------------
// Olusturma
// ---------------------------------------------------------------------------

pub async fn create(
    State(state): State<AppState>,
    user: AuthUser,
    Path(wid): Path<String>,
    Json(req): Json<CreateItemReq>,
) -> AppResult<(axum::http::StatusCode, Json<WorkItemRow>)> {
    let ctx = WsCtx::load(&state.db, &user, &wid).await?;
    ctx.require("work_item.create")?;

    let name = req.name.trim().to_string();
    if name.is_empty() || name.len() > 120 {
        return Err(AppError::BadRequest("Is kalemi adi 1-120 karakter olmali".into()));
    }
    let priority = req.priority.unwrap_or_else(|| "medium".into());
    validate_priority(&priority)?;
    let status = req.status.unwrap_or_else(|| "draft".into());
    validate_status(&status)?;
    if let Some(d) = &req.planned_start {
        validate_date(d)?;
    }
    if let Some(d) = &req.planned_end {
        validate_date(d)?;
    }

    load_section_checked(&state.db, &ctx, &wid, &req.section_id).await?;

    // Is tipi dogrula + tanimlari yukle
    let defs = match &req.work_type_id {
        Some(tid) => {
            let ok: Option<(String,)> = sqlx::query_as(
                "SELECT id FROM work_types WHERE id = ?1 AND workspace_id = ?2 AND archived_at IS NULL",
            )
            .bind(tid)
            .bind(&wid)
            .fetch_optional(&state.db)
            .await?;
            if ok.is_none() {
                return Err(AppError::BadRequest("Gecersiz is tipi".into()));
            }
            defs_for_type(&state.db, tid).await?
        }
        None => HashMap::new(),
    };

    let id = util::new_id();
    let now = util::now();

    let mut tx = state.db.begin().await?;
    sqlx::query(
        "INSERT INTO work_items
         (id, workspace_id, section_id, work_type_id, name, description, priority, status, planned_start, planned_end, created_by, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?12)",
    )
    .bind(&id)
    .bind(&wid)
    .bind(&req.section_id)
    .bind(&req.work_type_id)
    .bind(&name)
    .bind(&req.description)
    .bind(&priority)
    .bind(&status)
    .bind(&req.planned_start)
    .bind(&req.planned_end)
    .bind(&user.0.id)
    .bind(&now)
    .execute(&mut *tx)
    .await?;

    if !req.attributes.is_empty() || !defs.is_empty() {
        upsert_attribute_values(&mut tx, &id, &defs, &req.attributes, &user.0.id, true).await?;
    }
    tx.commit().await?;

    // Tipin varsayilan akisi varsa otomatik ata (gercek senaryo paketi)
    // Surec grubu secilmisse dogrudan ata (tip varsayilini one alir)
    if let Some(tid) = &req.workflow_template_id {
        assign_workflow_to_item(&state.db, &wid, &id, tid, &user.0.id).await;
    } else if req.auto_workflow {
        if let Some(tid) = &req.work_type_id {
            auto_assign_workflow(&state.db, &wid, &id, tid, &user.0.id).await;
        }
    }

    let row = fetch_row(&state.db, &wid, &id).await?;
    Ok((axum::http::StatusCode::CREATED, Json(row)))
}

/// Is tipinin varsayilan akisi varsa yayinlanmis son versiyonunu bulur.
/// Donus: (template_id, version_id) — akis yoksa None.
pub async fn default_workflow_for_type(
    db: &sqlx::SqlitePool,
    work_type_id: &str,
) -> Option<(String, String)> {
    let t: Option<(Option<String>,)> = sqlx::query_as(
        "SELECT default_workflow_template_id FROM work_types WHERE id = ?1 AND archived_at IS NULL",
    )
    .bind(work_type_id)
    .fetch_optional(db)
    .await
    .ok()
    .flatten();
    let Some((Some(tfid),)) = t else { return None };
    let v: Option<(String,)> = sqlx::query_as(
        "SELECT v.id FROM workflow_versions v
         WHERE v.template_id = ?1 AND v.status = 'published'
         ORDER BY v.version_number DESC LIMIT 1",
    )
    .bind(&tfid)
    .fetch_optional(db)
    .await
    .ok()
    .flatten();
    let (vid,) = v?;
    Some((tfid, vid))
}

/// Akis atamasi olusturur (workflow_runtime::assign ile ayni mantik, programatik).
/// Hata halinde sessizce atlar (otomatik atama best-effort).
pub async fn auto_assign_workflow(
    db: &sqlx::SqlitePool,
    wid: &str,
    iid: &str,
    work_type_id: &str,
    user_id: &str,
) -> bool {
    let Some((_, version_id)) = default_workflow_for_type(db, work_type_id).await else {
        return false;
    };
    spawn_if_free(db, wid, iid, &version_id, user_id).await
}

/// Secili surec grubunun (template) yayinlanmis son versiyonunu is kalemine atar.
/// Grubu olusturan islemlerde kullanilir; aktif instance varsa dokunmaz.
pub async fn assign_workflow_to_item(
    db: &sqlx::SqlitePool,
    wid: &str,
    iid: &str,
    template_id: &str,
    user_id: &str,
) -> bool {
    // Template bu workspace'e mi ait + published versiyon
    let v: Option<(String,)> = sqlx::query_as(
        "SELECT v.id FROM workflow_versions v
         JOIN workflow_templates t ON t.id = v.template_id
         WHERE v.template_id = ?1 AND t.workspace_id = ?2 AND v.status = 'published'
           AND t.status = 'active'
         ORDER BY v.version_number DESC LIMIT 1",
    )
    .bind(template_id)
    .bind(wid)
    .fetch_optional(db)
    .await
    .ok()
    .flatten();
    let Some((version_id,)) = v else { return false };
    spawn_if_free(db, wid, iid, &version_id, user_id).await
}

/// Aktif instance yoksa spawn; varsa dokunma (best-effort).
async fn spawn_if_free(
    db: &sqlx::SqlitePool,
    wid: &str,
    iid: &str,
    version_id: &str,
    user_id: &str,
) -> bool {
    let active: Option<(String,)> = sqlx::query_as(
        "SELECT id FROM workflow_instances WHERE work_item_id = ?1 AND status = 'active'",
    )
    .bind(iid)
    .fetch_optional(db)
    .await
    .ok()
    .flatten();
    if active.is_some() {
        return false;
    }
    crate::workflow_runtime::spawn_instance(db, wid, iid, version_id, user_id)
        .await
        .is_ok()
}

#[derive(Deserialize)]
pub struct DistributeReq {
    parent_section_id: String,
    /// leaves: alt bolumu olmayan tum bolumlere; children: dogrudan alt bolumlere
    #[serde(default = "default_target")]
    target: String,
    work_type_id: Option<String>,
    name: String,
    /// Tipin varsayilan akisi varsa otomatik ata
    #[serde(default)]
    auto_workflow: bool,
    /// Secili surec grubunu (workflow template) her olusan kaleme ata
    #[serde(default)]
    pub workflow_template_id: Option<String>,
    #[serde(default)]
    priority: Option<String>,
    #[serde(default)]
    status: Option<String>,
}

fn default_target() -> String {
    "leaves".into()
}

/// Seri olusturma (yol haritasi 42) - tek transaction, default degerlerle.
pub async fn create_bulk(
    State(state): State<AppState>,
    user: AuthUser,
    Path(wid): Path<String>,
    Json(req): Json<BulkCreateReq>,
) -> AppResult<Json<serde_json::Value>> {
    let ctx = WsCtx::load(&state.db, &user, &wid).await?;
    ctx.require("work_item.create")?;

    if req.start < 0 || req.end < req.start || (req.end - req.start + 1) > 500 {
        return Err(AppError::BadRequest("Aralik gecersiz ya da cok buyuk (max 500)".into()));
    }
    if !req.template.contains("{n") {
        return Err(AppError::BadRequest("Sablon icinde {n}/{nn}/{nnn} kullanilmali".into()));
    }
    let priority = req.priority.unwrap_or_else(|| "medium".into());
    validate_priority(&priority)?;
    let status = req.status.unwrap_or_else(|| "draft".into());
    validate_status(&status)?;

    // Kapsam + varlik kontrolu, bolum adini da al (sablon {parent} icin)
    load_section_checked(&state.db, &ctx, &wid, &req.section_id).await?;
    let section_name: Option<(String,)> = sqlx::query_as(
        "SELECT name FROM sections WHERE id = ?1 AND workspace_id = ?2 AND archived_at IS NULL",
    )
    .bind(&req.section_id)
    .bind(&wid)
    .fetch_optional(&state.db)
    .await?;
    let pname = section_name.map(|(n,)| n);

    // Tip tanimlari (default degerler icin)
    let defs = match &req.work_type_id {
        Some(tid) => {
            let ok: Option<(String,)> = sqlx::query_as(
                "SELECT id FROM work_types WHERE id = ?1 AND workspace_id = ?2 AND archived_at IS NULL",
            )
            .bind(tid)
            .bind(&wid)
            .fetch_optional(&state.db)
            .await?;
            if ok.is_none() {
                return Err(AppError::BadRequest("Gecersiz is tipi".into()));
            }
            defs_for_type(&state.db, tid).await?
        }
        None => HashMap::new(),
    };

    // Default degerleri hazirla
    let default_values: Vec<AttributeValueIn> = defs
        .values()
        .filter_map(|d| {
            d.default_value.as_ref().map(|dv| {
                let v: serde_json::Value =
                    serde_json::from_str(dv).unwrap_or(serde_json::Value::String(dv.clone()));
                AttributeValueIn {
                    definition_id: d.id.clone(),
                    value: v,
                }
            })
        })
        .collect();

    // Zorunlu alan default'u yoksa seri olusturulamaz
    for d in defs.values() {
        if d.is_required
            && !default_values
                .iter()
                .any(|v| v.definition_id == d.id)
        {
            return Err(AppError::BadRequest(format!(
                "Zorunlu ozellik '{}' icin tip taniminda default deger yok - seri olusturma yapilamaz",
                d.name
            )));
        }
    }

    let now = util::now();
    let mut tx = state.db.begin().await?;
    let mut created = 0i64;

    for n in req.start..=req.end {
        let name = util::render_serial_name(&req.template, n as usize, pname.as_deref());
        if name.trim().is_empty() {
            continue;
        }
        let id = util::new_id();
        sqlx::query(
            "INSERT INTO work_items
             (id, workspace_id, section_id, work_type_id, name, priority, status, created_by, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?9)",
        )
        .bind(&id)
        .bind(&wid)
        .bind(&req.section_id)
        .bind(&req.work_type_id)
        .bind(&name)
        .bind(&priority)
        .bind(&status)
        .bind(&user.0.id)
        .bind(&now)
        .execute(&mut *tx)
        .await?;

        if !default_values.is_empty() {
            upsert_attribute_values(&mut tx, &id, &defs, &default_values, &user.0.id, false).await?;
        }
        created += 1;
    }
    tx.commit().await?;
    Ok(Json(serde_json::json!({ "created": created })))
}

/// Bolumlere dagitimli is kalemi (gercek senaryo paketi):
/// Katlar kokune "Mutfak Tezgahi" -> her Daire'ye birer is kalemi.
/// target=leaves: alt bolumu olmayan tum bolumler; children: dogrudan alt bolumler.
pub async fn create_distribute(
    State(state): State<AppState>,
    user: AuthUser,
    Path(wid): Path<String>,
    Json(req): Json<DistributeReq>,
) -> AppResult<Json<serde_json::Value>> {
    let ctx = WsCtx::load(&state.db, &user, &wid).await?;
    ctx.require("work_item.create")?;

    if !matches!(req.target.as_str(), "leaves" | "children") {
        return Err(AppError::BadRequest("target leaves|children olmali".into()));
    }
    let priority = req.priority.unwrap_or_else(|| "medium".into());
    validate_priority(&priority)?;
    let status = req.status.unwrap_or_else(|| "draft".into());
    validate_status(&status)?;
    let name_tpl = req.name.trim().to_string();
    if name_tpl.is_empty() || name_tpl.len() > 160 {
        return Err(AppError::BadRequest("Isim 1-160 karakter olmali".into()));
    }

    // Kok bolum: kapsam + path
    let root_path = load_section_checked(&state.db, &ctx, &wid, &req.parent_section_id).await?;

    // Hedef bolumler (tek sorgu; subtree + target + kapsam Rust filtresi)
    let rows: Vec<(String, String, String)> = sqlx::query_as(
        "SELECT s.id, s.name, s.path_cache
         FROM sections s
         WHERE s.workspace_id = ?1 AND s.archived_at IS NULL
           AND (s.path_cache LIKE ?2 || '/%' OR s.id = ?3)",
    )
    .bind(&wid)
    .bind(&root_path)
    .bind(&req.parent_section_id)
    .fetch_all(&state.db)
    .await?;

    let root_depth = root_path.matches('/').count();
    let is_parent_of_any = |id: &str| {
        // child path'i ".../id/child" biciminde icerir (path = /kok/.../id/child)
        rows.iter().any(|(_, _, p)| p.contains(&format!("/{id}/")))
    };

    let targets: Vec<(String, String)> = rows
        .iter()
        .filter(|(id, _, path)| {
            if *id == req.parent_section_id {
                return false;
            }
            if !ctx.path_in_scope(path) {
                return false;
            }
            let depth = path.matches('/').count();
            match req.target.as_str() {
                "leaves" => !is_parent_of_any(id),
                _ => depth == root_depth + 1,
            }
        })
        .map(|(id, name, _)| (id.clone(), name.clone()))
        .collect();

    if targets.is_empty() {
        return Err(AppError::BadRequest("Hedef bolum bulunamadi".into()));
    }
    if targets.len() > 500 {
        return Err(AppError::BadRequest("Cok fazla hedef (max 500)".into()));
    }

    let now = util::now();
    let mut tx = state.db.begin().await?;
    let mut auto_ids: Vec<(String, Option<String>)> = Vec::new();

    for (i, (sid, sname)) in targets.iter().enumerate() {
        let name = util::render_serial_name(&name_tpl, i + 1, Some(sname));
        let id = util::new_id();
        sqlx::query(
            "INSERT INTO work_items
             (id, workspace_id, section_id, work_type_id, name, priority, status, created_by, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?9)",
        )
        .bind(&id)
        .bind(&wid)
        .bind(sid)
        .bind(&req.work_type_id)
        .bind(&name)
        .bind(&priority)
        .bind(&status)
        .bind(&user.0.id)
        .bind(&now)
        .execute(&mut *tx)
        .await?;
        if req.workflow_template_id.is_some() || req.auto_workflow {
            auto_ids.push((id, req.work_type_id.clone()));
        }
    }
    tx.commit().await?;

    // Otomatik akis atamalari — tx DISINDA (tek baglanti kilidi dersi, Faz 5)
    // Surec grubu secilmisse grup one alir; degilse tipin varsayilini dener.
    let mut auto_count = 0i64;
    for (iid, tid) in &auto_ids {
        let assigned = if let Some(gid) = &req.workflow_template_id {
            assign_workflow_to_item(&state.db, &wid, iid, gid, &user.0.id).await
        } else if let Some(tid) = tid {
            auto_assign_workflow(&state.db, &wid, iid, tid, &user.0.id).await
        } else {
            false
        };
        if assigned {
            auto_count += 1;
        }
    }

    Ok(Json(serde_json::json!({
        "created": targets.len(),
        "auto_assigned": auto_count
    })))
}

// ---------------------------------------------------------------------------
// Detay / guncelleme
// ---------------------------------------------------------------------------

async fn fetch_row(
    db: &sqlx::SqlitePool,
    wid: &str,
    iid: &str,
) -> AppResult<WorkItemRow> {
    sqlx::query_as::<_, WorkItemRow>(
        format!("SELECT w.id, w.section_id, s.name AS section_name, w.work_type_id, t.name AS work_type_name,
                w.name, w.description, w.priority, w.status, w.planned_start, w.planned_end, w.created_at{ITEM_PROGRESS}
         FROM work_items w
         JOIN sections s ON s.id = w.section_id
         LEFT JOIN work_types t ON t.id = w.work_type_id
         WHERE w.id = ?1 AND w.workspace_id = ?2 AND w.archived_at IS NULL").as_str(),
    )
    .bind(iid)
    .bind(wid)
    .fetch_optional(db)
    .await?
    .ok_or_else(|| AppError::NotFound("Is kalemi bulunamadi".into()))
}

pub async fn get_one(
    State(state): State<AppState>,
    user: AuthUser,
    Path((wid, iid)): Path<(String, String)>,
) -> AppResult<Json<WorkItemDetail>> {
    let ctx = WsCtx::load(&state.db, &user, &wid).await?;
    let base = fetch_row(&state.db, &wid, &iid).await?;

    // Kapsam kontrolu: section path
    let section: Option<(String,)> = sqlx::query_as(
        "SELECT path_cache FROM sections WHERE id = ?1",
    )
    .bind(&base.section_id)
    .fetch_optional(&state.db)
    .await?;
    if let Some((path,)) = section {
        if !ctx.path_in_scope(&path) {
            return Err(AppError::Forbidden("Kapsam disi is kalemi".into()));
        }
    }

    // Bolum yolu (isim dizisi)
    let all_sections: Vec<(String, Option<String>, String)> = sqlx::query_as(
        "SELECT id, parent_id, name FROM sections WHERE workspace_id = ?1 AND archived_at IS NULL",
    )
    .bind(&wid)
    .fetch_all(&state.db)
    .await?;
    let by_id: HashMap<String, (Option<String>, String)> = all_sections
        .into_iter()
        .map(|(id, p, n)| (id, (p, n)))
        .collect();
    let mut path_names = Vec::new();
    let mut cur = base.section_id.clone();
    while let Some((parent, name)) = by_id.get(&cur) {
        path_names.push(name.clone());
        match parent {
            Some(p) => cur = p.clone(),
            None => break,
        }
    }
    path_names.reverse();

    // Ozellikler (tanim + deger birlesik)
    let mut attrs_out = Vec::new();
    if let Some(tid) = &base.work_type_id {
        let defs = attributes::load_definitions_with_options(&state.db, tid).await?;
        let values: Vec<(String, Option<String>, Option<f64>, Option<i64>, Option<String>, Option<String>, Option<String>)> = sqlx::query_as(
            "SELECT attribute_definition_id, value_text, value_number, value_boolean, value_date, value_datetime, value_json
             FROM work_attribute_values WHERE work_item_id = ?1",
        )
        .bind(&iid)
        .fetch_all(&state.db)
        .await?;

        for d in defs {
            let v = values.iter().find(|(def_id, ..)| def_id == &d.id);
            let value = match v {
                None | Some((_, None, None, None, None, None, None)) => serde_json::Value::Null,
                Some((_, Some(t), ..)) if d.data_type != "boolean" && matches!(d.data_type.as_str(), "text" | "textarea" | "email" | "phone" | "select") => serde_json::json!(t),
                Some((_, _, Some(n), ..)) if matches!(d.data_type.as_str(), "integer" | "decimal" | "currency" | "percentage") => {
                    if d.data_type == "integer" {
                        serde_json::json!(n.round() as i64)
                    } else {
                        serde_json::json!(n)
                    }
                }
                Some((_, _, _, Some(b), ..)) => serde_json::json!(*b == 1),
                Some((_, _, _, _, Some(date), ..)) if d.data_type == "date" => serde_json::json!(date),
                Some((_, _, _, _, _, Some(dt), ..)) if d.data_type == "datetime" => serde_json::json!(dt),
                Some((_, _, _, _, _, _, Some(j))) if d.data_type == "multiselect" => {
                    serde_json::from_str(j).unwrap_or(serde_json::Value::Null)
                }
                _ => serde_json::Value::Null,
            };
            attrs_out.push(AttributeValueOut {
                definition_id: d.id,
                key: d.key,
                name: d.name,
                data_type: d.data_type,
                unit: None,
                is_required: d.is_required,
                value,
                options: d.options,
            });
        }
    }

    Ok(Json(WorkItemDetail {
        base,
        section_path: path_names,
        attributes: attrs_out,
    }))
}

pub async fn update(
    State(state): State<AppState>,
    user: AuthUser,
    Path((wid, iid)): Path<(String, String)>,
    Json(req): Json<UpdateItemReq>,
) -> AppResult<Json<WorkItemRow>> {
    let ctx = WsCtx::load(&state.db, &user, &wid).await?;
    ctx.require("work_item.update")?;

    let existing = fetch_row(&state.db, &wid, &iid).await?;
    let section: Option<(String,)> =
        sqlx::query_as("SELECT path_cache FROM sections WHERE id = ?1")
            .bind(&existing.section_id)
            .fetch_optional(&state.db)
            .await?;
    if let Some((path,)) = section {
        if !ctx.path_in_scope(&path) {
            return Err(AppError::Forbidden("Kapsam disi is kalemi".into()));
        }
    }

    if let Some(p) = &req.priority {
        validate_priority(p)?;
    }
    if let Some(s) = &req.status {
        validate_status(s)?;
    }
    if let Some(d) = &req.planned_start {
        validate_date(d)?;
    }
    if let Some(d) = &req.planned_end {
        validate_date(d)?;
    }
    if let Some(n) = &req.name {
        let n = n.trim();
        if n.is_empty() || n.len() > 120 {
            return Err(AppError::BadRequest("Is kalemi adi 1-120 karakter olmali".into()));
        }
    }
    // bolum degisimi: kapsam kontrolu
    if let Some(sid) = &req.section_id {
        if *sid != existing.section_id {
            load_section_checked(&state.db, &ctx, &wid, sid).await?;
        }
    }
    // tip degisimi: gecerli mi
    if let Some(tid) = &req.work_type_id {
        let ok: Option<(String,)> = sqlx::query_as(
            "SELECT id FROM work_types WHERE id = ?1 AND workspace_id = ?2 AND archived_at IS NULL",
        )
        .bind(tid)
        .bind(&wid)
        .fetch_optional(&state.db)
        .await?;
        if ok.is_none() {
            return Err(AppError::BadRequest("Gecersiz is tipi".into()));
        }
    }

    let now = util::now();
    let mut tx = state.db.begin().await?;
    if let Some(n) = &req.name {
        sqlx::query("UPDATE work_items SET name = ?1 WHERE id = ?2")
            .bind(n.trim())
            .bind(&iid)
            .execute(&mut *tx)
            .await?;
    }
    if let Some(d) = &req.description {
        sqlx::query("UPDATE work_items SET description = ?1 WHERE id = ?2")
            .bind(d)
            .bind(&iid)
            .execute(&mut *tx)
            .await?;
    }
    if let Some(s) = &req.section_id {
        sqlx::query("UPDATE work_items SET section_id = ?1 WHERE id = ?2")
            .bind(s)
            .bind(&iid)
            .execute(&mut *tx)
            .await?;
    }
    if let Some(t) = &req.work_type_id {
        sqlx::query("UPDATE work_items SET work_type_id = ?1 WHERE id = ?2")
            .bind(t)
            .bind(&iid)
            .execute(&mut *tx)
            .await?;
    }
    if let Some(p) = &req.priority {
        sqlx::query("UPDATE work_items SET priority = ?1 WHERE id = ?2")
            .bind(p)
            .bind(&iid)
            .execute(&mut *tx)
            .await?;
    }
    if let Some(s) = &req.status {
        sqlx::query("UPDATE work_items SET status = ?1 WHERE id = ?2")
            .bind(s)
            .bind(&iid)
            .execute(&mut *tx)
            .await?;
    }
    if let Some(d) = &req.planned_start {
        sqlx::query("UPDATE work_items SET planned_start = ?1 WHERE id = ?2")
            .bind(d)
            .bind(&iid)
            .execute(&mut *tx)
            .await?;
    }
    if let Some(d) = &req.planned_end {
        sqlx::query("UPDATE work_items SET planned_end = ?1 WHERE id = ?2")
            .bind(d)
            .bind(&iid)
            .execute(&mut *tx)
            .await?;
    }
    sqlx::query("UPDATE work_items SET updated_at = ?1 WHERE id = ?2")
        .bind(&now)
        .bind(&iid)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;

    let row = fetch_row(&state.db, &wid, &iid).await?;
    Ok(Json(row))
}

pub async fn archive(
    State(state): State<AppState>,
    user: AuthUser,
    Path((wid, iid)): Path<(String, String)>,
) -> AppResult<Json<serde_json::Value>> {
    let ctx = WsCtx::load(&state.db, &user, &wid).await?;
    ctx.require("work_item.archive")?;

    let existing = fetch_row(&state.db, &wid, &iid).await?;
    let section: Option<(String,)> =
        sqlx::query_as("SELECT path_cache FROM sections WHERE id = ?1")
            .bind(&existing.section_id)
            .fetch_optional(&state.db)
            .await?;
    if let Some((path,)) = section {
        if !ctx.path_in_scope(&path) {
            return Err(AppError::Forbidden("Kapsam disi is kalemi".into()));
        }
    }

    let now = util::now();
    sqlx::query("UPDATE work_items SET archived_at = ?1, updated_at = ?1 WHERE id = ?2")
        .bind(&now)
        .bind(&iid)
        .execute(&state.db)
        .await?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

/// Ozellik degerlerini toplu guncelle (PATCH attributes).
pub async fn update_attributes(
    State(state): State<AppState>,
    user: AuthUser,
    Path((wid, iid)): Path<(String, String)>,
    Json(req): Json<serde_json::Value>,
) -> AppResult<Json<serde_json::Value>> {
    let ctx = WsCtx::load(&state.db, &user, &wid).await?;
    ctx.require("work_item.update")?;

    let existing = fetch_row(&state.db, &wid, &iid).await?;
    let section: Option<(String,)> =
        sqlx::query_as("SELECT path_cache FROM sections WHERE id = ?1")
            .bind(&existing.section_id)
            .fetch_optional(&state.db)
            .await?;
    if let Some((path,)) = section {
        if !ctx.path_in_scope(&path) {
            return Err(AppError::Forbidden("Kapsam disi is kalemi".into()));
        }
    }

    #[derive(Deserialize)]
    struct AttrUpdateReq {
        #[serde(default)]
        attributes: Vec<AttributeValueIn>,
    }
    let parsed: AttrUpdateReq =
        serde_json::from_value(req).map_err(|_| AppError::BadRequest("Gecersiz istek govdesi".into()))?;

    let tid = existing
        .work_type_id
        .clone()
        .ok_or_else(|| AppError::BadRequest("Bu is kaleminin tipi yok; ozellik olamaz".into()))?;
    let defs = defs_for_type(&state.db, &tid).await?;

    let mut tx = state.db.begin().await?;
    // Kismi guncelleme: verilen degerler uzerinden zorunluluk kontrolu
    // (tum zorunlu alanlar onceki kayitlarda mevcut kabul edilir)
    upsert_attribute_values(&mut tx, &iid, &defs, &parsed.attributes, &user.0.id, false).await?;
    tx.commit().await?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/workspaces/{wid}/work-items",
            routing::get(list).post(create),
        )
        .route("/workspaces/{wid}/work-items/bulk", routing::post(create_bulk))
        .route("/workspaces/{wid}/work-items/bulk-distribute", routing::post(create_distribute))
        .route(
            "/workspaces/{wid}/work-items/{iid}",
            routing::get(get_one).patch(update),
        )
        .route(
            "/workspaces/{wid}/work-items/{iid}/archive",
            routing::post(archive),
        )
        .route(
            "/workspaces/{wid}/work-items/{iid}/attributes",
            routing::patch(update_attributes),
        )
}
