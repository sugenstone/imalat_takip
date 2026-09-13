use std::collections::{HashMap, HashSet, VecDeque};

use axum::extract::{Path, State};
use axum::{routing, Json, Router};
use serde::{Deserialize, Serialize};

use crate::auth::AuthUser;
use crate::error::{AppError, AppResult};
use crate::state::AppState;
use crate::util;
use crate::ws_ctx::WsCtx;

// ---------------------------------------------------------------------------
// Tipler
// ---------------------------------------------------------------------------

#[derive(Serialize, sqlx::FromRow)]
pub struct WorkflowRow {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub status: String,
    /// Yayindaki versiyon numarasi (yoksa null)
    pub published_version: Option<i64>,
    /// Yayindaki versiyonun adim sayisi
    pub published_node_count: Option<i64>,
    /// Aktif draft'taki adim sayisi
    pub draft_node_count: Option<i64>,
}

#[derive(Serialize, sqlx::FromRow)]
pub struct VersionRow {
    pub id: String,
    pub version_number: i64,
    pub status: String,
    pub published_at: Option<String>,
    pub node_count: i64,
}

#[derive(Serialize, sqlx::FromRow)]
pub struct NodeRow {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub node_type: String,
    pub sort_order: i64,
    pub approval_rule: Option<String>, // JSON: {required, approver_role_id}
    pub default_assignee_type: Option<String>, // user | team
    pub default_assignee_id: Option<String>,
}

#[derive(Serialize)]
pub struct DependencyRow {
    pub id: String,
    pub predecessor_node_id: String,
    pub successor_node_id: String,
    pub dependency_type: String,
}

#[derive(Serialize)]
pub struct DraftOut {
    pub version_id: String,
    pub version_number: i64,
    pub nodes: Vec<NodeRow>,
    pub dependencies: Vec<DependencyRow>,
}

#[derive(Deserialize)]
pub struct CreateTemplateReq {
    name: String,
    #[serde(default)]
    description: Option<String>,
}

#[derive(Deserialize)]
pub struct UpdateTemplateReq {
    name: String,
    #[serde(default)]
    description: Option<String>,
}

#[derive(Deserialize)]
pub struct CreateNodeReq {
    name: String,
    #[serde(default)]
    description: Option<String>,
}

#[derive(Deserialize)]
pub struct UpdateNodeReq {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    description: Option<String>,
    /// Yukari tasi (siralamayi degistirir)
    #[serde(default)]
    move_up: bool,
    /// Onay kurali: {required: bool, approver_role_id?: string}
    #[serde(default)]
    approval_rule: Option<serde_json::Value>,
    /// Varsayilan atanan: {"type": "user"|"team", "id": "..."} — null temizler
    #[serde(default)]
    default_assignee: Option<serde_json::Value>,
}

#[derive(Deserialize)]
pub struct CreateDependencyReq {
    predecessor_node_id: String,
    successor_node_id: String,
}

const WF_SELECT: &str = "SELECT t.id, t.name, t.description, t.status,
    (SELECT MAX(version_number) FROM workflow_versions v WHERE v.template_id = t.id AND v.status = 'published') AS published_version,
    (SELECT COUNT(*) FROM workflow_nodes n JOIN workflow_versions v2 ON v2.id = n.version_id
       WHERE v2.template_id = t.id AND v2.status = 'published' AND v2.version_number =
         (SELECT MAX(version_number) FROM workflow_versions WHERE template_id = t.id AND status = 'published')) AS published_node_count,
    (SELECT COUNT(*) FROM workflow_nodes n JOIN workflow_versions v3 ON v3.id = n.version_id
       WHERE v3.template_id = t.id AND v3.status = 'draft') AS draft_node_count
    FROM workflow_templates t";

// ---------------------------------------------------------------------------
// Yardimcilar
// ---------------------------------------------------------------------------

async fn load_template(
    db: &sqlx::SqlitePool,
    wid: &str,
    tfid: &str,
) -> AppResult<()> {
    let ok: Option<(String,)> = sqlx::query_as(
        "SELECT id FROM workflow_templates WHERE id = ?1 AND workspace_id = ?2 AND archived_at IS NULL",
    )
    .bind(tfid)
    .bind(wid)
    .fetch_optional(db)
    .await?;
    if ok.is_some() {
        Ok(())
    } else {
        Err(AppError::NotFound("Akis bulunamadi".into()))
    }
}

/// Aktif draft versiyonunu getir; yoksa olustur (version_number = max + 1).
async fn get_or_create_draft(
    db: &sqlx::SqlitePool,
    tfid: &str,
) -> AppResult<(String, i64)> {
    if let Some((vid, vn)) = sqlx::query_as::<_, (String, i64)>(
        "SELECT id, version_number FROM workflow_versions
         WHERE template_id = ?1 AND status = 'draft'
         ORDER BY version_number DESC LIMIT 1",
    )
    .bind(tfid)
    .fetch_optional(db)
    .await?
    {
        return Ok((vid, vn));
    }
    let max: Option<(i64,)> = sqlx::query_as(
        "SELECT MAX(version_number) FROM workflow_versions WHERE template_id = ?1",
    )
    .bind(tfid)
    .fetch_optional(db)
    .await?;
    let next = max.map(|(v,)| v).unwrap_or(0) + 1;

    let vid = util::new_id();
    sqlx::query(
        "INSERT INTO workflow_versions (id, template_id, version_number, status, created_at)
         VALUES (?1, ?2, ?3, 'draft', ?4)",
    )
    .bind(&vid)
    .bind(tfid)
    .bind(next)
    .bind(util::now())
    .execute(db)
    .await?;
    Ok((vid, next))
}

/// Draft versiyonunun node + bagimliliklarini yukler.
async fn load_draft_nodes(
    db: &sqlx::SqlitePool,
    version_id: &str,
) -> AppResult<(Vec<NodeRow>, Vec<DependencyRow>)> {
    let nodes = sqlx::query_as::<_, NodeRow>(
        "SELECT id, name, description, node_type, sort_order, approval_rule_json AS approval_rule,
                default_assignee_type, default_assignee_id
         FROM workflow_nodes WHERE version_id = ?1 ORDER BY sort_order, created_at",
    )
    .bind(version_id)
    .fetch_all(db)
    .await?;

    let deps = sqlx::query_as::<_, (String, String, String, String)>(
        "SELECT id, predecessor_node_id, successor_node_id, dependency_type
         FROM workflow_dependencies WHERE version_id = ?1",
    )
    .bind(version_id)
    .fetch_all(db)
    .await?;
    let deps = deps
        .into_iter()
        .map(|(id, p, s, t)| DependencyRow {
            id,
            predecessor_node_id: p,
            successor_node_id: s,
            dependency_type: t,
        })
        .collect();
    Ok((nodes, deps))
}

/// Grafigi adjacency listeye cevir: predecessor -> [successors]
fn build_adjacency(deps: &[DependencyRow]) -> HashMap<String, Vec<String>> {
    let mut adj: HashMap<String, Vec<String>> = HashMap::new();
    for d in deps {
        adj.entry(d.predecessor_node_id.clone())
            .or_default()
            .push(d.successor_node_id.clone());
    }
    adj
}

/// `from` node'undan `target` node'una yol var mi (BFS)?
fn reaches(adj: &HashMap<String, Vec<String>>, from: &str, target: &str) -> bool {
    if from == target {
        return true;
    }
    let mut seen: HashSet<String> = HashSet::new();
    let mut q = VecDeque::new();
    q.push_back(from.to_string());
    seen.insert(from.to_string());
    while let Some(cur) = q.pop_front() {
        if let Some(nexts) = adj.get(&cur) {
            for n in nexts {
                if n == target {
                    return true;
                }
                if seen.insert(n.clone()) {
                    q.push_back(n.clone());
                }
            }
        }
    }
    false
}

/// Tum grafta cycle var mi (iteratif renkli DFS: 0=beyaz, 1=gri, 2=siyah)?
fn has_cycle(adj: &HashMap<String, Vec<String>>, all_nodes: &[String]) -> bool {
    let mut color: HashMap<String, u8> = HashMap::new();
    for n in all_nodes {
        color.insert(n.clone(), 0);
    }
    for start in all_nodes {
        if color.get(start).copied().unwrap_or(0) != 0 {
            continue;
        }
        let mut stack: Vec<(String, usize)> = Vec::new();
        color.insert(start.clone(), 1);
        stack.push((start.clone(), 0));
        while let Some((node, idx)) = stack.last_mut() {
            let children = adj.get(node.as_str()).cloned().unwrap_or_default();
            if *idx >= children.len() {
                let (node, _) = stack.pop().unwrap();
                color.insert(node, 2);
                continue;
            }
            let child = children[*idx].clone();
            *idx += 1;
            let c = color.get(&child).copied().unwrap_or(0);
            if c == 1 {
                return true; // gri node'a tekrar ulasildi -> cycle
            }
            if c == 0 {
                color.insert(child.clone(), 1);
                stack.push((child, 0));
            }
        }
    }
    false
}

// ---------------------------------------------------------------------------
// Template CRUD
// ---------------------------------------------------------------------------

pub async fn list_templates(
    State(state): State<AppState>,
    user: AuthUser,
    Path(wid): Path<String>,
) -> AppResult<Json<Vec<WorkflowRow>>> {
    WsCtx::load(&state.db, &user, &wid).await?;
    let rows = sqlx::query_as::<_, WorkflowRow>(&format!(
        "{WF_SELECT} WHERE t.workspace_id = ?1 AND t.archived_at IS NULL ORDER BY t.name"
    ))
    .bind(&wid)
    .fetch_all(&state.db)
    .await?;
    Ok(Json(rows))
}

pub async fn create_template(
    State(state): State<AppState>,
    user: AuthUser,
    Path(wid): Path<String>,
    Json(req): Json<CreateTemplateReq>,
) -> AppResult<(axum::http::StatusCode, Json<WorkflowRow>)> {
    let ctx = WsCtx::load(&state.db, &user, &wid).await?;
    ctx.require("workflow.create")?;

    let name = req.name.trim().to_string();
    if name.is_empty() || name.len() > 80 {
        return Err(AppError::BadRequest("Akis adi 1-80 karakter olmali".into()));
    }
    let exists: Option<(String,)> = sqlx::query_as(
        "SELECT id FROM workflow_templates WHERE workspace_id = ?1 AND name = ?2 AND archived_at IS NULL",
    )
    .bind(&wid)
    .bind(&name)
    .fetch_optional(&state.db)
    .await?;
    if exists.is_some() {
        return Err(AppError::Conflict("Bu isimde akis zaten var".into()));
    }

    let id = util::new_id();
    let now = util::now();
    sqlx::query(
        "INSERT INTO workflow_templates (id, workspace_id, name, description, status, created_by, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, 'active', ?5, ?6, ?6)",
    )
    .bind(&id)
    .bind(&wid)
    .bind(&name)
    .bind(&req.description)
    .bind(&user.0.id)
    .bind(&now)
    .execute(&state.db)
    .await?;

    let row = sqlx::query_as::<_, WorkflowRow>(&format!("{WF_SELECT} WHERE t.id = ?1"))
        .bind(&id)
        .fetch_one(&state.db)
        .await?;
    Ok((axum::http::StatusCode::CREATED, Json(row)))
}

pub async fn update_template(
    State(state): State<AppState>,
    user: AuthUser,
    Path((wid, tfid)): Path<(String, String)>,
    Json(req): Json<UpdateTemplateReq>,
) -> AppResult<Json<WorkflowRow>> {
    let ctx = WsCtx::load(&state.db, &user, &wid).await?;
    ctx.require("workflow.update")?;
    load_template(&state.db, &wid, &tfid).await?;

    let name = req.name.trim().to_string();
    if name.is_empty() || name.len() > 80 {
        return Err(AppError::BadRequest("Akis adi 1-80 karakter olmali".into()));
    }
    sqlx::query(
        "UPDATE workflow_templates SET name = ?1, description = ?2, updated_at = ?3 WHERE id = ?4",
    )
    .bind(&name)
    .bind(&req.description)
    .bind(util::now())
    .bind(&tfid)
    .execute(&state.db)
    .await?;

    let row = sqlx::query_as::<_, WorkflowRow>(&format!("{WF_SELECT} WHERE t.id = ?1"))
        .bind(&tfid)
        .fetch_one(&state.db)
        .await?;
    Ok(Json(row))
}

pub async fn archive_template(
    State(state): State<AppState>,
    user: AuthUser,
    Path((wid, tfid)): Path<(String, String)>,
) -> AppResult<Json<serde_json::Value>> {
    let ctx = WsCtx::load(&state.db, &user, &wid).await?;
    ctx.require("workflow.update")?;
    load_template(&state.db, &wid, &tfid).await?;

    let now = util::now();
    sqlx::query(
        "UPDATE workflow_templates SET archived_at = ?1, updated_at = ?1, status = 'archived' WHERE id = ?2",
    )
    .bind(&now)
    .bind(&tfid)
    .execute(&state.db)
    .await?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

// ---------------------------------------------------------------------------
// Draft yonetimi
// ---------------------------------------------------------------------------

/// Aktif draft'i dondurur; ilk erisimde bos draft olusturur.
pub async fn get_draft(
    State(state): State<AppState>,
    user: AuthUser,
    Path((wid, tfid)): Path<(String, String)>,
) -> AppResult<Json<DraftOut>> {
    WsCtx::load(&state.db, &user, &wid).await?;
    load_template(&state.db, &wid, &tfid).await?;

    let (vid, vn) = get_or_create_draft(&state.db, &tfid).await?;
    let (nodes, deps) = load_draft_nodes(&state.db, &vid).await?;
    Ok(Json(DraftOut {
        version_id: vid,
        version_number: vn,
        nodes,
        dependencies: deps,
    }))
}

pub async fn create_node(
    State(state): State<AppState>,
    user: AuthUser,
    Path((wid, tfid)): Path<(String, String)>,
    Json(req): Json<CreateNodeReq>,
) -> AppResult<(axum::http::StatusCode, Json<NodeRow>)> {
    let ctx = WsCtx::load(&state.db, &user, &wid).await?;
    ctx.require("workflow.update")?;
    load_template(&state.db, &wid, &tfid).await?;

    let name = req.name.trim().to_string();
    if name.is_empty() || name.len() > 80 {
        return Err(AppError::BadRequest("Adim adi 1-80 karakter olmali".into()));
    }

    let (vid, _) = get_or_create_draft(&state.db, &tfid).await?;
    let max: Option<(i64,)> = sqlx::query_as(
        "SELECT MAX(sort_order) FROM workflow_nodes WHERE version_id = ?1",
    )
    .bind(&vid)
    .fetch_optional(&state.db)
    .await?;
    let sort = max.map(|(v,)| v).unwrap_or(0) + 1;

    let id = util::new_id();
    sqlx::query(
        "INSERT INTO workflow_nodes (id, version_id, name, description, node_type, sort_order, created_at)
         VALUES (?1, ?2, ?3, ?4, 'process', ?5, ?6)",
    )
    .bind(&id)
    .bind(&vid)
    .bind(&name)
    .bind(&req.description)
    .bind(sort)
    .bind(util::now())
    .execute(&state.db)
    .await?;

    Ok((
        axum::http::StatusCode::CREATED,
        Json(NodeRow {
            id,
            name,
            description: req.description,
            node_type: "process".into(),
            sort_order: sort,
            approval_rule: None,
            default_assignee_type: None,
            default_assignee_id: None,
        }),
    ))
}

pub async fn update_node(
    State(state): State<AppState>,
    user: AuthUser,
    Path((wid, tfid, nid)): Path<(String, String, String)>,
    Json(req): Json<UpdateNodeReq>,
) -> AppResult<Json<serde_json::Value>> {
    let ctx = WsCtx::load(&state.db, &user, &wid).await?;
    ctx.require("workflow.update")?;
    load_template(&state.db, &wid, &tfid).await?;
    let (vid, _) = get_or_create_draft(&state.db, &tfid).await?;

    let node: Option<(i64,)> = sqlx::query_as(
        "SELECT sort_order FROM workflow_nodes WHERE id = ?1 AND version_id = ?2",
    )
    .bind(&nid)
    .bind(&vid)
    .fetch_optional(&state.db)
    .await?;
    let (cur_sort,) = node.ok_or_else(|| AppError::NotFound("Adim bulunamadi".into()))?;

    if let Some(n) = &req.name {
        let n = n.trim();
        if n.is_empty() || n.len() > 80 {
            return Err(AppError::BadRequest("Adim adi 1-80 karakter olmali".into()));
        }
        sqlx::query("UPDATE workflow_nodes SET name = ?1 WHERE id = ?2")
            .bind(n)
            .bind(&nid)
            .execute(&state.db)
            .await?;
    }
    if let Some(d) = &req.description {
        sqlx::query("UPDATE workflow_nodes SET description = ?1 WHERE id = ?2")
            .bind(d)
            .bind(&nid)
            .execute(&state.db)
            .await?;
    }

    // Onay kurali (Faz 6): {required: bool, approver_role_id?: string}
    if let Some(rule) = &req.approval_rule {
        let required = rule.get("required").and_then(|b| b.as_bool()).unwrap_or(false);
        let approver_role = rule
            .get("approver_role_id")
            .and_then(|r| r.as_str())
            .map(|s| s.to_string());
        if required {
            if let Some(rid) = &approver_role {
                let ok: Option<(String,)> = sqlx::query_as(
                    "SELECT id FROM roles WHERE id = ?1 AND workspace_id = ?2",
                )
                .bind(rid)
                .bind(&wid)
                .fetch_optional(&state.db)
                .await?;
                if ok.is_none() {
                    return Err(AppError::BadRequest("Gecersiz onay rolu".into()));
                }
            }
            let json = serde_json::json!({
                "required": true,
                "approver_role_id": approver_role
            });
            sqlx::query("UPDATE workflow_nodes SET approval_rule_json = ?1 WHERE id = ?2")
                .bind(json.to_string())
                .bind(&nid)
                .execute(&state.db)
                .await?;
        } else {
            sqlx::query("UPDATE workflow_nodes SET approval_rule_json = NULL WHERE id = ?1")
                .bind(&nid)
                .execute(&state.db)
                .await?;
        }
    }

    // Varsayilan atanan (user | team) — null temizler
    if let Some(assignee) = &req.default_assignee {
        if assignee.is_null() {
            sqlx::query("UPDATE workflow_nodes SET default_assignee_type = NULL, default_assignee_id = NULL WHERE id = ?1")
                .bind(&nid)
                .execute(&state.db)
                .await?;
        } else {
            let atype = assignee.get("type").and_then(|v| v.as_str()).unwrap_or("");
            let aid = assignee.get("id").and_then(|v| v.as_str()).unwrap_or("");
            if !matches!(atype, "user" | "team") || aid.is_empty() {
                return Err(AppError::BadRequest("default_assignee: {type: user|team, id} gerekli".into()));
            }
            if atype == "user" {
                let ok: Option<(String,)> = sqlx::query_as(
                    "SELECT id FROM workspace_members WHERE workspace_id = ?1 AND user_id = ?2 AND status = 'active' AND archived_at IS NULL",
                )
                .bind(&wid)
                .bind(aid)
                .fetch_optional(&state.db)
                .await?;
                if ok.is_none() {
                    return Err(AppError::BadRequest("Kullanici bu workspace'in uyesi degil".into()));
                }
            } else {
                let ok: Option<(String,)> = sqlx::query_as(
                    "SELECT id FROM teams WHERE id = ?1 AND workspace_id = ?2 AND archived_at IS NULL",
                )
                .bind(aid)
                .bind(&wid)
                .fetch_optional(&state.db)
                .await?;
                if ok.is_none() {
                    return Err(AppError::BadRequest("Takim bulunamadi".into()));
                }
            }
            sqlx::query("UPDATE workflow_nodes SET default_assignee_type = ?1, default_assignee_id = ?2 WHERE id = ?3")
                .bind(atype)
                .bind(aid)
                .bind(&nid)
                .execute(&state.db)
                .await?;
        }
    }

    // Siralama tasi: komsu ile sort_order degis tokusu
    if req.move_up {
        let neighbor: Option<(String,)> = sqlx::query_as(
            "SELECT id FROM workflow_nodes WHERE version_id = ?1 AND sort_order < ?2
             ORDER BY sort_order DESC LIMIT 1",
        )
        .bind(&vid)
        .bind(cur_sort)
        .fetch_optional(&state.db)
        .await?;
        if let Some((nb,)) = neighbor {
            let nb_sort: (i64,) = sqlx::query_as(
                "SELECT sort_order FROM workflow_nodes WHERE id = ?1",
            )
            .bind(&nb)
            .fetch_one(&state.db)
            .await?;
            let mut tx = state.db.begin().await?;
            sqlx::query("UPDATE workflow_nodes SET sort_order = ?1 WHERE id = ?2")
                .bind(cur_sort)
                .bind(&nb)
                .execute(&mut *tx)
                .await?;
            sqlx::query("UPDATE workflow_nodes SET sort_order = ?1 WHERE id = ?2")
                .bind(nb_sort.0)
                .bind(&nid)
                .execute(&mut *tx)
                .await?;
            tx.commit().await?;
        }
    }

    Ok(Json(serde_json::json!({ "ok": true })))
}

/// Adimi arsivler (draft'ten cikarir); bagimliliklari cascade silinir.
pub async fn archive_node(
    State(state): State<AppState>,
    user: AuthUser,
    Path((wid, tfid, nid)): Path<(String, String, String)>,
) -> AppResult<Json<serde_json::Value>> {
    let ctx = WsCtx::load(&state.db, &user, &wid).await?;
    ctx.require("workflow.update")?;
    load_template(&state.db, &wid, &tfid).await?;
    let (vid, _) = get_or_create_draft(&state.db, &tfid).await?;

    let res = sqlx::query("DELETE FROM workflow_nodes WHERE id = ?1 AND version_id = ?2")
        .bind(&nid)
        .bind(&vid)
        .execute(&state.db)
        .await?;
    if res.rows_affected() == 0 {
        return Err(AppError::NotFound("Adim bulunamadi".into()));
    }
    Ok(Json(serde_json::json!({ "ok": true })))
}

/// Bagimlilik ekle - EKLEME ANINDA cycle kontrolu (yol haritasi 23).
pub async fn create_dependency(
    State(state): State<AppState>,
    user: AuthUser,
    Path((wid, tfid)): Path<(String, String)>,
    Json(req): Json<CreateDependencyReq>,
) -> AppResult<(axum::http::StatusCode, Json<serde_json::Value>)> {
    let ctx = WsCtx::load(&state.db, &user, &wid).await?;
    ctx.require("workflow.update")?;
    load_template(&state.db, &wid, &tfid).await?;
    let (vid, _) = get_or_create_draft(&state.db, &tfid).await?;

    if req.predecessor_node_id == req.successor_node_id {
        return Err(AppError::BadRequest("Adim kendisine baglanamaz".into()));
    }
    for nid in [&req.predecessor_node_id, &req.successor_node_id] {
        let ok: Option<(String,)> =
            sqlx::query_as("SELECT id FROM workflow_nodes WHERE id = ?1 AND version_id = ?2")
                .bind(nid)
                .bind(&vid)
                .fetch_optional(&state.db)
                .await?;
        if ok.is_none() {
            return Err(AppError::BadRequest("Adim bu draft'a ait degil".into()));
        }
    }

    let exists: Option<(String,)> = sqlx::query_as(
        "SELECT id FROM workflow_dependencies WHERE predecessor_node_id = ?1 AND successor_node_id = ?2",
    )
    .bind(&req.predecessor_node_id)
    .bind(&req.successor_node_id)
    .fetch_optional(&state.db)
    .await?;
    if exists.is_some() {
        return Err(AppError::Conflict("Bu bagimlilik zaten var".into()));
    }

    // Cycle kontrolu: successor'dan predecessor'a zaten yol varsa bu edge cycle olusturur.
    let (_, deps) = load_draft_nodes(&state.db, &vid).await?;
    let adj = build_adjacency(&deps);
    if reaches(&adj, &req.successor_node_id, &req.predecessor_node_id) {
        return Err(AppError::BadRequest(
            "Bu bagimlilik dongu (cycle) olusturur".into(),
        ));
    }

    let id = util::new_id();
    sqlx::query(
        "INSERT INTO workflow_dependencies (id, version_id, predecessor_node_id, successor_node_id, dependency_type, created_at)
         VALUES (?1, ?2, ?3, ?4, 'all_completed', ?5)",
    )
    .bind(&id)
    .bind(&vid)
    .bind(&req.predecessor_node_id)
    .bind(&req.successor_node_id)
    .bind(util::now())
    .execute(&state.db)
    .await?;
    Ok((axum::http::StatusCode::CREATED, Json(serde_json::json!({ "id": id }))))
}

pub async fn delete_dependency(
    State(state): State<AppState>,
    user: AuthUser,
    Path((wid, tfid, dep_id)): Path<(String, String, String)>,
) -> AppResult<Json<serde_json::Value>> {
    let ctx = WsCtx::load(&state.db, &user, &wid).await?;
    ctx.require("workflow.update")?;
    load_template(&state.db, &wid, &tfid).await?;
    let (vid, _) = get_or_create_draft(&state.db, &tfid).await?;

    let res = sqlx::query("DELETE FROM workflow_dependencies WHERE id = ?1 AND version_id = ?2")
        .bind(&dep_id)
        .bind(&vid)
        .execute(&state.db)
        .await?;
    if res.rows_affected() == 0 {
        return Err(AppError::NotFound("Bagimlilik bulunamadi".into()));
    }
    Ok(Json(serde_json::json!({ "ok": true })))
}

// ---------------------------------------------------------------------------
// Publish + versiyonlar
// ---------------------------------------------------------------------------

/// Draft'i yayinlar: final cycle kontrolu -> draft published olur -> YENI bos draft olusur.
/// Published versiyon bir daha degistirilemez (immutable).
pub async fn publish(
    State(state): State<AppState>,
    user: AuthUser,
    Path((wid, tfid)): Path<(String, String)>,
) -> AppResult<Json<serde_json::Value>> {
    let ctx = WsCtx::load(&state.db, &user, &wid).await?;
    ctx.require("workflow.publish")?;
    load_template(&state.db, &wid, &tfid).await?;

    let (vid, vn) = get_or_create_draft(&state.db, &tfid).await?;
    let (nodes, deps) = load_draft_nodes(&state.db, &vid).await?;

    if nodes.is_empty() {
        return Err(AppError::BadRequest(
            "Yayinlamak icin en az bir adim gerekli".into(),
        ));
    }

    // Final cycle kontrolu (yol haritasi 23)
    let all_ids: Vec<String> = nodes.iter().map(|n| n.id.clone()).collect();
    let adj = build_adjacency(&deps);
    if has_cycle(&adj, &all_ids) {
        return Err(AppError::BadRequest(
            "Akista dongu (cycle) var; bagimliliklari duzeltin".into(),
        ));
    }

    let now = util::now();
    let mut tx = state.db.begin().await?;
    sqlx::query(
        "UPDATE workflow_versions SET status = 'published', published_at = ?1, published_by = ?2 WHERE id = ?3",
    )
    .bind(&now)
    .bind(&user.0.id)
    .bind(&vid)
    .execute(&mut *tx)
    .await?;

    // Yeni bos draft
    let new_vid = util::new_id();
    sqlx::query(
        "INSERT INTO workflow_versions (id, template_id, version_number, status, created_at)
         VALUES (?1, ?2, ?3, 'draft', ?4)",
    )
    .bind(&new_vid)
    .bind(&tfid)
    .bind(vn + 1)
    .bind(&now)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;

    crate::audit::audit(&state.db, &wid, Some(&user.0.id), "workflow.published", "workflow_template", &tfid,
        serde_json::json!({ "version": vn, "node_count": nodes.len() })).await;
    Ok(Json(serde_json::json!({ "published_version": vn, "next_draft_version": vn + 1 })))
}

pub async fn list_versions(
    State(state): State<AppState>,
    user: AuthUser,
    Path((wid, tfid)): Path<(String, String)>,
) -> AppResult<Json<Vec<VersionRow>>> {
    WsCtx::load(&state.db, &user, &wid).await?;
    load_template(&state.db, &wid, &tfid).await?;

    let rows = sqlx::query_as::<_, VersionRow>(
        "SELECT v.id, v.version_number, v.status, v.published_at,
            (SELECT COUNT(*) FROM workflow_nodes n WHERE n.version_id = v.id) AS node_count
         FROM workflow_versions v
         WHERE v.template_id = ?1
         ORDER BY v.version_number DESC",
    )
    .bind(&tfid)
    .fetch_all(&state.db)
    .await?;
    Ok(Json(rows))
}


// ---------------------------------------------------------------------------
// Hizli akis olusturma (quick-flow): tek cagrida olustur + zincirle + yayinla + ata
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct QuickStep {
    name: String,
    #[serde(default)]
    requires_approval: bool,
    /// Onay kurali rolu (roles.id) — verilmezse onay yetkisi olan herkes onaylar
    #[serde(default)]
    approver_role_id: Option<String>,
}

#[derive(Deserialize)]
pub struct QuickFlowReq {
    name: String,
    steps: Vec<QuickStep>,
    /// Verildiyse yayinladiktan sonra bu is kalemine otomatik ata
    #[serde(default)]
    assign_to_item_id: Option<String>,
}

/// Hizli akis: adimlari sirali zincir olarak kurar, yayinlar, (istendiyse) is kalemine atar.
/// Paralel dallar ve detayli kurallar icin Ayarlar > Akislar'daki gelismis editor gecerlidir.
pub async fn quick_flow(
    State(state): State<AppState>,
    user: AuthUser,
    Path(wid): Path<String>,
    Json(req): Json<QuickFlowReq>,
) -> AppResult<Json<serde_json::Value>> {
    let ctx = WsCtx::load(&state.db, &user, &wid).await?;
    ctx.require("workflow.create")?;

    let name = req.name.trim().to_string();
    if name.is_empty() || name.len() > 80 {
        return Err(AppError::BadRequest("Akis adi 1-80 karakter olmali".into()));
    }
    if req.steps.is_empty() || req.steps.len() > 20 {
        return Err(AppError::BadRequest("1-20 adim gerekli".into()));
    }
    for st in &req.steps {
        let n = st.name.trim();
        if n.is_empty() || n.len() > 80 {
            return Err(AppError::BadRequest("Adim adi 1-80 karakter olmali".into()));
        }
    }

    // Is kalemi atamasi yapilacaksa kapsam kontrolu
    if let Some(iid) = &req.assign_to_item_id {
        let row: Option<(String,)> = sqlx::query_as(
            "SELECT s.path_cache FROM work_items w JOIN sections s ON s.id = w.section_id
             WHERE w.id = ?1 AND w.workspace_id = ?2 AND w.archived_at IS NULL",
        )
        .bind(iid)
        .bind(&wid)
        .fetch_optional(&state.db)
        .await?;
        match row {
            Some((path,)) if ctx.path_in_scope(&path) => {}
            Some(_) => return Err(AppError::Forbidden("Kapsam disi is kalemi".into())),
            None => return Err(AppError::NotFound("Is kalemi bulunamadi".into())),
        }
    }

    let now = util::now();
    let tfid = util::new_id();
    let mut tx = state.db.begin().await?;

    // 1) Template
    sqlx::query(
        "INSERT INTO workflow_templates (id, workspace_id, name, status, created_by, created_at, updated_at)
         VALUES (?1, ?2, ?3, 'active', ?4, ?5, ?5)",
    )
    .bind(&tfid)
    .bind(&wid)
    .bind(&name)
    .bind(&user.0.id)
    .bind(&now)
    .execute(&mut *tx)
    .await?;

    // 2) Versiyon (draft) + adimlar + zincir bagimliliklar
    let version_id = util::new_id();
    sqlx::query(
        "INSERT INTO workflow_versions (id, template_id, version_number, status, created_at)
         VALUES (?1, ?2, 1, 'draft', ?3)",
    )
    .bind(&version_id)
    .bind(&tfid)
    .bind(&now)
    .execute(&mut *tx)
    .await?;

    let mut prev_node: Option<String> = None;
    for (i, st) in req.steps.iter().enumerate() {
        let node_id = util::new_id();
        let approval = if st.requires_approval {
            serde_json::json!({ "required": true }).to_string()
        } else {
            "NULL".to_string()
        };
        let _ = approval;
        if st.requires_approval {
            let rule = serde_json::json!({
                "required": true,
                "approver_role_id": st.approver_role_id
            });
            sqlx::query(
                "INSERT INTO workflow_nodes (id, version_id, name, node_type, sort_order, approval_rule_json, created_at)
                 VALUES (?1, ?2, ?3, 'process', ?4, ?5, ?6)",
            )
            .bind(&node_id)
            .bind(&version_id)
            .bind(st.name.trim())
            .bind((i + 1) as i64)
            .bind(rule.to_string())
            .bind(&now)
            .execute(&mut *tx)
            .await?;
        } else {
            sqlx::query(
                "INSERT INTO workflow_nodes (id, version_id, name, node_type, sort_order, created_at)
                 VALUES (?1, ?2, ?3, 'process', ?4, ?5)",
            )
            .bind(&node_id)
            .bind(&version_id)
            .bind(st.name.trim())
            .bind((i + 1) as i64)
            .bind(&now)
            .execute(&mut *tx)
            .await?;
        }

        if let Some(p) = &prev_node {
            sqlx::query(
                "INSERT INTO workflow_dependencies (id, version_id, predecessor_node_id, successor_node_id, dependency_type, created_at)
                 VALUES (?1, ?2, ?3, ?4, 'all_completed', ?5)",
            )
            .bind(util::new_id())
            .bind(&version_id)
            .bind(p)
            .bind(&node_id)
            .bind(&now)
            .execute(&mut *tx)
            .await?;
        }
        prev_node = Some(node_id);
    }

    // 3) Yayinla
    sqlx::query(
        "UPDATE workflow_versions SET status = 'published', published_at = ?1, published_by = ?2 WHERE id = ?3",
    )
    .bind(&now)
    .bind(&user.0.id)
    .bind(&version_id)
    .execute(&mut *tx)
    .await?;

    // Yeni bos draft (v2) birak
    sqlx::query(
        "INSERT INTO workflow_versions (id, template_id, version_number, status, created_at)
         VALUES (?1, ?2, 2, 'draft', ?3)",
    )
    .bind(util::new_id())
    .bind(&tfid)
    .bind(&now)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;

    crate::audit::audit(&state.db, &wid, Some(&user.0.id), "workflow.published", "workflow_template", &tfid,
        serde_json::json!({ "version": 1, "via": "quick-flow", "steps": req.steps.len() })).await;

    // 4) Is kalemine ata (tx disinda — spawn_instance kendi tx acar)
    let mut assigned = false;
    if let Some(iid) = &req.assign_to_item_id {
        // Aktif instance var mi? Varsa atama yapma (kullanici degistirmek isterse once iptal etmeli)
        let active: Option<(String,)> = sqlx::query_as(
            "SELECT id FROM workflow_instances WHERE work_item_id = ?1 AND status = 'active'",
        )
        .bind(iid)
        .fetch_optional(&state.db)
        .await?;
        if active.is_none() {
            let inst = crate::workflow_runtime::spawn_instance(
                &state.db, &wid, iid, &version_id, &user.0.id,
            )
            .await?;
            let _ = inst;
            assigned = true;
            crate::audit::audit(&state.db, &wid, Some(&user.0.id), "workflow.assigned", "work_item", iid,
                serde_json::json!({ "template": name, "version": 1, "via": "quick-flow" })).await;
        }
    }

    Ok(Json(serde_json::json!({
        "template_id": tfid,
        "published_version": 1,
        "assigned": assigned
    })))
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/workspaces/{wid}/workflows",
            routing::get(list_templates).post(create_template),
        )
        .route(
            "/workspaces/{wid}/workflows/{tfid}",
            routing::patch(update_template),
        )
        .route(
            "/workspaces/{wid}/workflows/{tfid}/archive",
            routing::post(archive_template),
        )
        .route(
            "/workspaces/{wid}/workflows/{tfid}/draft",
            routing::get(get_draft),
        )
        .route(
            "/workspaces/{wid}/workflows/{tfid}/draft/nodes",
            routing::post(create_node),
        )
        .route(
            "/workspaces/{wid}/workflows/{tfid}/draft/nodes/{nid}",
            routing::patch(update_node).delete(archive_node),
        )
        .route(
            "/workspaces/{wid}/workflows/{tfid}/draft/dependencies",
            routing::post(create_dependency),
        )
        .route(
            "/workspaces/{wid}/workflows/{tfid}/draft/dependencies/{dep_id}",
            routing::delete(delete_dependency),
        )
        .route(
            "/workspaces/{wid}/workflows/{tfid}/publish",
            routing::post(publish),
        )
        .route("/workspaces/{wid}/quick-flow", routing::post(quick_flow))
        .route(
            "/workspaces/{wid}/workflows/{tfid}/versions",
            routing::get(list_versions),
        )
}
