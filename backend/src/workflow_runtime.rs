//! Faz 5-6: Workflow Runtime — instance yonetimi, surec motoru, atama ve onay.
//!
//! Motor (yol haritasi 40-41): submit/approve sonrasi dependency'ler yeniden
//! degerlendirilir; TUM predecessor'lari onaylanan successor'lar ready olur.
//! Tum surecler terminal durumda ise instance completed olur.
//!
//! Faz 6 (yol haritasi 26-27): submit != approve. Onay kurali olan node'lar
//! submitted'da bekler; approve/reject karari sonrasi cascade devam eder.

use axum::extract::{Path, State};
use axum::{routing, Json, Router};
use serde::{Deserialize, Serialize};

use crate::auth::AuthUser;
use crate::error::{AppError, AppResult};
use crate::state::AppState;
use crate::util;
use crate::ws_ctx::WsCtx;

#[derive(Serialize, sqlx::FromRow)]
pub struct ProcessOut {
    pub id: String,
    pub node_id: String,
    pub name: String,
    pub sort_order: i64,
    pub status: String,
    pub ready_at: Option<String>,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
    /// Bu adimin predecessor node id'leri
    pub predecessor_ids: Option<String>, // JSON array
    /// Aktif atamalar: [{id, type, assignee_id, assignee_name}]
    pub assignments: Option<String>, // JSON array
    /// Bekleyen/karar verilmis son onay: {decision, decided_by_name, note}
    pub approval: Option<String>, // JSON object
    /// Faz 7: toplam deneme sayisi
    pub attempt_count: i64,
    /// Faz 7: aktif deneme numarasi (attempt yoksa 0)
    pub current_attempt: i64,
}

#[derive(Serialize, sqlx::FromRow)]
pub struct ReworkCycleOut {
    pub id: String,
    pub restart_from_name: String,
    pub triggered_by_name: String,
    pub reason: String,
    pub created_by_name: String,
    pub created_at: String,
}

#[derive(Serialize)]
pub struct InstanceOut {
    pub id: String,
    pub status: String,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
    pub template_name: String,
    pub version_number: i64,
    pub processes: Vec<ProcessOut>,
    pub total: usize,
    pub approved: usize,
    pub rework_cycles: Vec<ReworkCycleOut>,
}

#[derive(Deserialize)]
pub struct AssignReq {
    template_id: String,
}

/// Is kaleminin section path'i uzerinden kapsam kontrolu.
async fn check_item_scope(
    db: &sqlx::SqlitePool,
    ctx: &WsCtx,
    wid: &str,
    iid: &str,
) -> AppResult<()> {
    let row: Option<(String,)> = sqlx::query_as(
        "SELECT s.path_cache FROM work_items w JOIN sections s ON s.id = w.section_id
         WHERE w.id = ?1 AND w.workspace_id = ?2 AND w.archived_at IS NULL",
    )
    .bind(iid)
    .bind(wid)
    .fetch_optional(db)
    .await?;
    match row {
        Some((path,)) => {
            if ctx.path_in_scope(&path) {
                Ok(())
            } else {
                Err(AppError::Forbidden("Kapsam disi is kalemi".into()))
            }
        }
        None => Err(AppError::NotFound("Is kalemi bulunamadi".into())),
    }
}

// ---------------------------------------------------------------------------
// Atama
// ---------------------------------------------------------------------------

/// Versiyondan bagimsiz instance + surecler + node default atamalari uretir.
/// Hem manuel assign hem tip otomatik atamasi buradan gecer (tek kaynak).
pub async fn spawn_instance(
    db: &sqlx::SqlitePool,
    wid: &str,
    iid: &str,
    version_id: &str,
    actor_user_id: &str,
) -> AppResult<String> {
    let nodes: Vec<(String, Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT id, default_assignee_type, default_assignee_id FROM workflow_nodes WHERE version_id = ?1 ORDER BY sort_order",
    )
    .bind(version_id)
    .fetch_all(db)
    .await?;
    if nodes.is_empty() {
        return Err(AppError::BadRequest("Akista adim yok".into()));
    }
    let deps: Vec<(String, String)> = sqlx::query_as(
        "SELECT predecessor_node_id, successor_node_id FROM workflow_dependencies WHERE version_id = ?1",
    )
    .bind(version_id)
    .fetch_all(db)
    .await?;
    let has_pred: std::collections::HashSet<&str> = deps.iter().map(|(_, s)| s.as_str()).collect();

    let now = util::now();
    let instance_id = util::new_id();
    let mut node_to_process: std::collections::HashMap<String, String> = std::collections::HashMap::new();

    let mut tx = db.begin().await?;
    sqlx::query(
        "INSERT INTO workflow_instances (id, work_item_id, workflow_version_id, status, started_at, created_at)
         VALUES (?1, ?2, ?3, 'active', ?4, ?4)",
    )
    .bind(&instance_id)
    .bind(iid)
    .bind(version_id)
    .bind(&now)
    .execute(&mut *tx)
    .await?;

    for (node_id, _, _) in &nodes {
        let is_root = !has_pred.contains(node_id.as_str());
        let status = if is_root { "ready" } else { "waiting" };
        let pid = util::new_id();
        sqlx::query(
            "INSERT INTO process_instances (id, workflow_instance_id, workflow_node_id, status, ready_at, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?5)",
        )
        .bind(&pid)
        .bind(&instance_id)
        .bind(node_id)
        .bind(&status)
        .bind(&now)
        .execute(&mut *tx)
        .await?;
        node_to_process.insert(node_id.clone(), pid);
    }
    // Akış bağlandı -> kalem aktif (draft/completed ise)
    sync_item_status(&mut tx, iid, "assigned", &now).await?;
    tx.commit().await?;

    // Node default atamalari (tx disinda — tek baglanti kilidi dersi)
    for (node_id, dtype, did) in &nodes {
        if let (Some(t), Some(id)) = (dtype, did) {
            if let Some(pid) = node_to_process.get(node_id) {
                let dup: Option<(String,)> = sqlx::query_as(
                    "SELECT id FROM process_assignments WHERE process_instance_id = ?1 AND assignee_type = ?2 AND assignee_id = ?3 AND unassigned_at IS NULL",
                )
                .bind(pid).bind(t).bind(id)
                .fetch_optional(db)
                .await?;
                if dup.is_none() {
                    let _ = sqlx::query(
                        "INSERT INTO process_assignments (id, process_instance_id, assignee_type, assignee_id, assigned_by, assigned_at)
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    )
                    .bind(util::new_id()).bind(pid).bind(t).bind(id)
                    .bind(actor_user_id).bind(&now)
                    .execute(db)
                    .await;

                    // Bildirim: atananlara
                    let (pname, iname) = process_context(db, pid).await;
                    let targets: Vec<String> = if t == "user" {
                        vec![id.clone()]
                    } else {
                        sqlx::query_as::<_, (String,)>(
                            "SELECT user_id FROM team_members WHERE team_id = ?1",
                        )
                        .bind(id)
                        .fetch_all(db)
                        .await
                        .unwrap_or_default()
                        .into_iter().map(|(u,)| u).collect()
                    };
                    crate::notifications::notify_users(
                        db, wid, &targets, "process.assigned",
                        &format!("Yeni görev: {pname}"),
                        &format!("\"{pname}\" adımı \"{iname}\" iş kaleminde size atandı."),
                        "process_instance", pid,
                    ).await;
                }
            }
        }
    }

    Ok(instance_id)
}

/// Is kalemine akis ata: latest published versiyon
pub async fn assign(
    State(state): State<AppState>,
    user: AuthUser,
    Path((wid, iid)): Path<(String, String)>,
    Json(req): Json<AssignReq>,
) -> AppResult<(axum::http::StatusCode, Json<InstanceOut>)> {
    let ctx = WsCtx::load(&state.db, &user, &wid).await?;
    ctx.require("workflow.assign")?;
    check_item_scope(&state.db, &ctx, &wid, &iid).await?;

    // Template'in latest published versiyonu
    let ver: Option<(String, i64, String)> = sqlx::query_as(
        "SELECT v.id, v.version_number, t.name
         FROM workflow_versions v
         JOIN workflow_templates t ON t.id = v.template_id
         WHERE v.template_id = ?1 AND v.status = 'published' AND t.workspace_id = ?2
         ORDER BY v.version_number DESC LIMIT 1",
    )
    .bind(&req.template_id)
    .bind(&wid)
    .fetch_optional(&state.db)
    .await?;
    let (version_id, version_number, template_name) = ver.ok_or_else(|| {
        AppError::BadRequest("Bu akisin yayinlanmis versiyonu yok".into())
    })?;

    // Aktif instance var mi?
    let active: Option<(String,)> = sqlx::query_as(
        "SELECT id FROM workflow_instances WHERE work_item_id = ?1 AND status = 'active'",
    )
    .bind(&iid)
    .fetch_optional(&state.db)
    .await?;
    if active.is_some() {
        return Err(AppError::Conflict(
            "Bu is kaleminin zaten aktif akisi var".into(),
        ));
    }

    let instance_id = spawn_instance(
        &state.db, &wid, &iid, &version_id, &user.0.id,
    )
    .await?;

    let out = load_instance(&state.db, &instance_id).await?;
    crate::audit::audit(&state.db, &wid, Some(&user.0.id), "workflow.assigned", "work_item", &iid,
        serde_json::json!({ "template": template_name, "version": version_number, "instance": instance_id })).await;
    Ok((axum::http::StatusCode::CREATED, Json(out)))
}

// ---------------------------------------------------------------------------
// Goruntuleme
// ---------------------------------------------------------------------------

async fn load_instance(db: &sqlx::SqlitePool, instance_id: &str) -> AppResult<InstanceOut> {
    let meta: Option<(String, Option<String>, Option<String>, String, i64)> = sqlx::query_as(
        "SELECT wi.status, wi.started_at, wi.completed_at, t.name, v.version_number
         FROM workflow_instances wi
         JOIN workflow_versions v ON v.id = wi.workflow_version_id
         JOIN workflow_templates t ON t.id = v.template_id
         WHERE wi.id = ?1",
    )
    .bind(instance_id)
    .fetch_optional(db)
    .await?;
    let (status, started_at, completed_at, template_name, version_number) =
        meta.ok_or_else(|| AppError::NotFound("Akis atamasi bulunamadi".into()))?;

    let processes = sqlx::query_as::<_, ProcessOut>(
        "SELECT p.id, p.workflow_node_id AS node_id, n.name, n.sort_order,
                p.status, p.ready_at, p.started_at, p.finished_at,
                (SELECT json_group_array(d.predecessor_node_id) FROM workflow_dependencies d
                 WHERE d.version_id = n.version_id AND d.successor_node_id = n.id) AS predecessor_ids,
                (SELECT json_group_array(json_object('id', pa.id, 'type', pa.assignee_type, 'assignee_id', pa.assignee_id,
                                                    'name', CASE WHEN pa.assignee_type = 'user' THEN
                                                        (SELECT u.name FROM users u WHERE u.id = pa.assignee_id)
                                                    ELSE (SELECT t.name FROM teams t WHERE t.id = pa.assignee_id) END))
                 FROM process_assignments pa
                 WHERE pa.process_instance_id = p.id AND pa.unassigned_at IS NULL) AS assignments,
                (SELECT json_object('decision', a.decision, 'note', a.note,
                                    'decided_by_name', (SELECT u2.name FROM users u2 WHERE u2.id = a.decided_by),
                                    'requested_by_name', (SELECT u3.name FROM users u3 WHERE u3.id = a.requested_by))
                 FROM approvals a WHERE a.process_instance_id = p.id
                 ORDER BY a.requested_at DESC LIMIT 1) AS approval,
                (SELECT COUNT(*) FROM process_attempts patt WHERE patt.process_instance_id = p.id) AS attempt_count,
                COALESCE((SELECT patt2.attempt_number FROM process_attempts patt2 WHERE patt2.id = p.current_attempt_id), 0) AS current_attempt
         FROM process_instances p
         JOIN workflow_nodes n ON n.id = p.workflow_node_id
         WHERE p.workflow_instance_id = ?1
         ORDER BY n.sort_order",
    )
    .bind(instance_id)
    .fetch_all(db)
    .await?;

    let rework_cycles = sqlx::query_as::<_, ReworkCycleOut>(
        "SELECT rc.id,
                (SELECT n.name FROM process_instances pi JOIN workflow_nodes n ON n.id = pi.workflow_node_id WHERE pi.id = rc.restart_from_process_id) AS restart_from_name,
                (SELECT n2.name FROM process_instances pi2 JOIN workflow_nodes n2 ON n2.id = pi2.workflow_node_id WHERE pi2.id = rc.triggered_by_process_id) AS triggered_by_name,
                rc.reason,
                (SELECT u.name FROM users u WHERE u.id = rc.created_by) AS created_by_name,
                rc.created_at
         FROM rework_cycles rc
         WHERE rc.workflow_instance_id = ?1
         ORDER BY rc.created_at DESC",
    )
    .bind(instance_id)
    .fetch_all(db)
    .await?;

    let approved = processes
        .iter()
        .filter(|p| p.status == "approved")
        .count();
    let total = processes.len();

    Ok(InstanceOut {
        id: instance_id.to_string(),
        status,
        started_at,
        completed_at,
        template_name,
        version_number,
        processes,
        total,
        approved,
        rework_cycles,
    })
}

/// Is kaleminin aktif (veya son) akis atamasini dondurur.
pub async fn get_assignment(
    State(state): State<AppState>,
    user: AuthUser,
    Path((wid, iid)): Path<(String, String)>,
) -> AppResult<Json<Option<InstanceOut>>> {
    let ctx = WsCtx::load(&state.db, &user, &wid).await?;
    check_item_scope(&state.db, &ctx, &wid, &iid).await?;

    let latest: Option<(String,)> = sqlx::query_as(
        "SELECT id FROM workflow_instances WHERE work_item_id = ?1 ORDER BY created_at DESC LIMIT 1",
    )
    .bind(&iid)
    .fetch_optional(&state.db)
    .await?;
    match latest {
        Some((instance_id,)) => Ok(Json(Some(load_instance(&state.db, &instance_id).await?))),
        None => Ok(Json(None)),
    }
}

// ---------------------------------------------------------------------------
// Surec islemleri (motor)
// ---------------------------------------------------------------------------

/// Sureci yukle + kapsam yetki kontrolu. (instance_id dondurur)
async fn load_process_checked(
    db: &sqlx::SqlitePool,
    ctx: &WsCtx,
    wid: &str,
    pid: &str,
) -> AppResult<(String, String)> {
    let row: Option<(String, String)> = sqlx::query_as(
        "SELECT p.status, p.workflow_instance_id FROM process_instances p WHERE p.id = ?1",
    )
    .bind(pid)
    .fetch_optional(db)
    .await?;
    let (status, instance_id) =
        row.ok_or_else(|| AppError::NotFound("Surec bulunamadi".into()))?;

    // Instance -> work item -> workspace + scope
    let wi: Option<(String, String)> = sqlx::query_as(
        "SELECT w.workspace_id, s.path_cache
         FROM workflow_instances wi
         JOIN work_items w ON w.id = wi.work_item_id
         JOIN sections s ON s.id = w.section_id
         WHERE wi.id = ?1",
    )
    .bind(&instance_id)
    .fetch_optional(db)
    .await?;
    let (ws_id, path) = wi.ok_or_else(|| AppError::NotFound("Surec baglami bozuk".into()))?;
    if ws_id != wid {
        return Err(AppError::NotFound("Surec bulunamadi".into()));
    }
    if !ctx.path_in_scope(&path) {
        return Err(AppError::Forbidden("Kapsam disi surec".into()));
    }
    let _ = status;
    Ok((status, instance_id))
}

/// ready -> in_progress
pub async fn start_process(
    State(state): State<AppState>,
    user: AuthUser,
    Path((wid, pid)): Path<(String, String)>,
    Json(_): Json<serde_json::Value>,
) -> AppResult<Json<serde_json::Value>> {
    let ctx = WsCtx::load(&state.db, &user, &wid).await?;
    ctx.require("process.start")?;
    let (status, _) = load_process_checked(&state.db, &ctx, &wid, &pid).await?;

    if status != "ready" {
        return Err(AppError::BadRequest(format!(
            "Surec 'Hazir' durumunda degil (mevcut: {status})"
        )));
    }
    let now = util::now();

    // Faz 7: her deneme ayri attempt kaydi (yol haritasi 29)
    let attempt_id = create_attempt(&state.db, &pid, &now).await?;

    sqlx::query(
        "UPDATE process_instances SET status = 'in_progress', started_at = ?1, current_attempt_id = ?2 WHERE id = ?3",
    )
    .bind(&now)
    .bind(&attempt_id)
    .bind(&pid)
    .execute(&state.db)
    .await?;
    crate::audit::audit(&state.db, &wid, Some(&user.0.id), "process.started", "process_instance", &pid,
        serde_json::json!({ "attempt": 1 })).await;
    Ok(Json(serde_json::json!({ "ok": true })))
}

/// Yeni attempt olusturur (attempt_number = max + 1) ve id dondurur.
async fn create_attempt(db: &sqlx::SqlitePool, pid: &str, now: &str) -> AppResult<String> {
    let max: Option<(i64,)> = sqlx::query_as(
        "SELECT MAX(attempt_number) FROM process_attempts WHERE process_instance_id = ?1",
    )
    .bind(pid)
    .fetch_optional(db)
    .await?;
    let next = max.map(|(v,)| v).unwrap_or(0) + 1;
    let id = util::new_id();
    sqlx::query(
        "INSERT INTO process_attempts (id, process_instance_id, attempt_number, status, started_at, created_at)
         VALUES (?1, ?2, ?3, 'in_progress', ?4, ?4)",
    )
    .bind(&id)
    .bind(pid)
    .bind(next)
    .bind(now)
    .execute(db)
    .await?;
    Ok(id)
}

/// in_progress -> submitted (onay kurali varsa) veya approved (kuralsiz) + cascade.
/// Faz 6 (yol haritasi 26): submit != approve — kurali olan node onay bekler.
pub async fn submit_process(
    State(state): State<AppState>,
    user: AuthUser,
    Path((wid, pid)): Path<(String, String)>,
    Json(_): Json<serde_json::Value>,
) -> AppResult<Json<serde_json::Value>> {
    let ctx = WsCtx::load(&state.db, &user, &wid).await?;
    ctx.require("process.complete")?;
    let (status, instance_id) = load_process_checked(&state.db, &ctx, &wid, &pid).await?;

    if status != "in_progress" {
        return Err(AppError::BadRequest(format!(
            "Surec 'Devam Ediyor' durumunda degil (mevcut: {status})"
        )));
    }

    // Node'un onay kurali var mi?
    let rule: Option<(Option<String>,)> = sqlx::query_as(
        "SELECT n.approval_rule_json FROM process_instances p
         JOIN workflow_nodes n ON n.id = p.workflow_node_id
         WHERE p.id = ?1",
    )
    .bind(&pid)
    .fetch_optional(&state.db)
    .await?;
    let requires_approval = rule
        .and_then(|(r,)| r)
        .and_then(|r| serde_json::from_str::<serde_json::Value>(&r).ok())
        .and_then(|v| v.get("required").and_then(|b| b.as_bool()))
        .unwrap_or(false);

    let now = util::now();
    let mut tx = state.db.begin().await?;

    if requires_approval {
        // Onay kurali var: submitted'da bekle + approval kaydi olustur
        sqlx::query(
            "UPDATE process_instances SET status = 'submitted' WHERE id = ?1",
        )
        .bind(&pid)
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "UPDATE process_attempts SET submitted_at = ?1 WHERE id = (SELECT current_attempt_id FROM process_instances WHERE id = ?2)",
        )
        .bind(&now)
        .bind(&pid)
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "INSERT INTO approvals (id, process_instance_id, requested_by, requested_at, decision)
             VALUES (?1, ?2, ?3, ?4, 'pending')",
        )
        .bind(util::new_id())
        .bind(&pid)
        .bind(&user.0.id)
        .bind(&now)
        .execute(&mut *tx)
        .await?;
    } else {
        // Kuralsiz: direkt approved + cascade (yol haritasi 26)
        sqlx::query(
            "UPDATE process_instances SET status = 'approved', finished_at = ?1 WHERE id = ?2",
        )
        .bind(&now)
        .bind(&pid)
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "UPDATE process_attempts SET submitted_at = ?1, approved_at = ?1, status = 'approved', completed_by = ?2
             WHERE id = (SELECT current_attempt_id FROM process_instances WHERE id = ?3)",
        )
        .bind(&now)
        .bind(&user.0.id)
        .bind(&pid)
        .execute(&mut *tx)
        .await?;
        promote_ready_successors(&mut tx, &instance_id, &now).await?;
        complete_instance_if_done(&mut tx, &instance_id, &now).await?;
    }
    tx.commit().await?;

    // Bildirim: onay rolune (Faz 9) — pool sorgulari tx KAPANDIKTAN sonra yapilmali
    if requires_approval {
        let (pname, iname) = process_context(&state.db, &pid).await;
        let mut approvers: Vec<String> = Vec::new();
        if let Some(rid) = load_approver_role(&state.db, &pid).await? {
            approvers = crate::notifications::users_with_role(&state.db, &wid, &rid).await;
        }
        crate::notifications::notify_users(
            &state.db, &wid, &approvers, "process.approval_required",
            &format!("Onay bekliyor: {pname}"),
            &format!("\"{iname}\" iş kaleminde \"{pname}\" adımı onayınızı bekliyor."),
            "process_instance", &pid,
        ).await;
    }

    Ok(Json(serde_json::json!({
        "ok": true,
        "status": if requires_approval { "submitted" } else { "approved" }
    })))
}

/// Instance icinde terminal olmayan surec kalmadiysa completed yapar.
async fn complete_instance_if_done(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    instance_id: &str,
    now: &str,
) -> AppResult<()> {
    let remaining: Option<(i64,)> = sqlx::query_as(
        "SELECT COUNT(*) FROM process_instances
         WHERE workflow_instance_id = ?1 AND status NOT IN ('approved', 'cancelled', 'skipped', 'failed')",
    )
    .bind(instance_id)
    .fetch_optional(&mut **tx)
    .await?;
    if remaining.map(|(c,)| c).unwrap_or(0) == 0 {
        sqlx::query(
            "UPDATE workflow_instances SET status = 'completed', completed_at = ?1 WHERE id = ?2",
        )
        .bind(now)
        .bind(instance_id)
        .execute(&mut **tx)
        .await?;
        // Akış bitti -> kalem tamamlandı (iptal edilmiş kalem hariç)
        let iid: Option<(String,)> = sqlx::query_as(
            "SELECT work_item_id FROM workflow_instances WHERE id = ?1",
        )
        .bind(instance_id)
        .fetch_optional(&mut **tx)
        .await?;
        if let Some((iid,)) = iid {
            sync_item_status(tx, &iid, "completed", now).await?;
        }
    }
    Ok(())
}

/// Akış yaşam döngüsünü iş kalemi durumuna yansıtır (tek yönlü senkron):
/// - assigned  : akış bağlandı / rework yeniden başladı -> draft|completed => active
/// - completed : akış tamamlandı                          => completed (cancelled kalem hariç)
/// - cancelled : akış iptal edildi (kalem akış bekler)    => active => draft
/// Elle konan 'blocked'/'cancelled' kalem durumları assign dışında ezilmez.
async fn sync_item_status(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    iid: &str,
    event: &str,
    now: &str,
) -> AppResult<()> {
    let sql = match event {
        "assigned" => {
            "UPDATE work_items SET status = 'active', updated_at = ?1
             WHERE id = ?2 AND status IN ('draft', 'completed')"
        }
        "completed" => {
            "UPDATE work_items SET status = 'completed', updated_at = ?1
             WHERE id = ?2 AND status NOT IN ('cancelled', 'completed')"
        }
        _ => {
            "UPDATE work_items SET status = 'draft', updated_at = ?1
             WHERE id = ?2 AND status = 'active'"
        }
    };
    sqlx::query(sql).bind(now).bind(iid).execute(&mut **tx).await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Onay (Faz 6 — yol haritasi 27)
// ---------------------------------------------------------------------------

#[derive(Deserialize, Default)]
pub struct DecisionReq {
    #[serde(default)]
    note: Option<String>,
}

/// Onay kuralini (approver_role_id) surecin node'undan okur.
async fn load_approver_role(db: &sqlx::SqlitePool, pid: &str) -> AppResult<Option<String>> {
    let rule: Option<(Option<String>,)> = sqlx::query_as(
        "SELECT n.approval_rule_json FROM process_instances p
         JOIN workflow_nodes n ON n.id = p.workflow_node_id WHERE p.id = ?1",
    )
    .bind(pid)
    .fetch_optional(db)
    .await?;
    Ok(rule
        .and_then(|(r,)| r)
        .and_then(|r| serde_json::from_str::<serde_json::Value>(&r).ok())
        .and_then(|v: serde_json::Value| {
            v.get("approver_role_id")
                .and_then(|r| r.as_str())
                .map(|s| s.to_string())
        }))
}

/// submitted -> approved + cascade. Yetki: process.approve + kural rolu (veya Owner).
pub async fn approve_process(
    State(state): State<AppState>,
    user: AuthUser,
    Path((wid, pid)): Path<(String, String)>,
    Json(req): Json<DecisionReq>,
) -> AppResult<Json<serde_json::Value>> {
    let ctx = WsCtx::load(&state.db, &user, &wid).await?;
    ctx.require("process.approve")?;
    let (status, instance_id) = load_process_checked(&state.db, &ctx, &wid, &pid).await?;

    if status != "submitted" {
        return Err(AppError::BadRequest(format!(
            "Surec onay beklemiyor (mevcut: {status})"
        )));
    }

    // Rol kisiti: kuralda rol belirtilmisse onaylayan o role sahip olmali (Owner her zaman onaylayabilir)
    if !ctx.is_owner {
        if let Some(required_role) = load_approver_role(&state.db, &pid).await? {
            let member_role: Option<(String,)> = sqlx::query_as(
                "SELECT r.id FROM workspace_members m JOIN roles r ON r.id = m.role_id
                 WHERE m.id = ?1",
            )
            .bind(&ctx.member_id)
            .fetch_optional(&state.db)
            .await?;
            if member_role.map(|(r,)| r) != Some(required_role) {
                return Err(AppError::Forbidden(
                    "Bu sureci yalnizca tanimli onay rolu onaylayabilir".into(),
                ));
            }
        }
    }

    let now = util::now();
    let mut tx = state.db.begin().await?;
    sqlx::query(
        "UPDATE process_instances SET status = 'approved', finished_at = ?1 WHERE id = ?2",
    )
    .bind(&now)
    .bind(&pid)
    .execute(&mut *tx)
    .await?;
    // Aktif attempt'i onayla (Faz 7)
    sqlx::query(
        "UPDATE process_attempts SET approved_at = ?1, status = 'approved', completed_by = ?2
         WHERE id = (SELECT current_attempt_id FROM process_instances WHERE id = ?3)",
    )
    .bind(&now)
    .bind(&user.0.id)
    .bind(&pid)
    .execute(&mut *tx)
    .await?;
    // Bekleyen approval kaydini karara bagla
    sqlx::query(
        "UPDATE approvals SET decision = 'approved', decided_by = ?1, decided_at = ?2, note = COALESCE(?3, note)
         WHERE process_instance_id = ?4 AND decision = 'pending'",
    )
    .bind(&user.0.id)
    .bind(&now)
    .bind(&req.note)
    .bind(&pid)
    .execute(&mut *tx)
    .await?;

    promote_ready_successors(&mut tx, &instance_id, &now).await?;
    complete_instance_if_done(&mut tx, &instance_id, &now).await?;
    tx.commit().await?;
    crate::audit::audit(&state.db, &wid, Some(&user.0.id), "process.approved", "process_instance", &pid,
        serde_json::json!({ "note": req.note })).await;
    // Bildirim: atananlara (Faz 9)
    {
        let (pname, iname) = process_context(&state.db, &pid).await;
        let targets = crate::notifications::process_assignee_users(&state.db, &pid).await;
        crate::notifications::notify_users(
            &state.db, &wid, &targets, "process.approved",
            &format!("Onaylandı: {pname}"),
            &format!("\"{iname}\" iş kalemindeki \"{pname}\" adımı onaylandı."),
            "process_instance", &pid,
        ).await;
    }
    Ok(Json(serde_json::json!({ "ok": true })))
}

/// submitted -> failed (kalici). Red notu zorunlu. Rework Faz 7'de.
pub async fn reject_process(
    State(state): State<AppState>,
    user: AuthUser,
    Path((wid, pid)): Path<(String, String)>,
    Json(req): Json<DecisionReq>,
) -> AppResult<Json<serde_json::Value>> {
    let ctx = WsCtx::load(&state.db, &user, &wid).await?;
    ctx.require("process.approve")?;
    let (status, _) = load_process_checked(&state.db, &ctx, &wid, &pid).await?;

    if status != "submitted" {
        return Err(AppError::BadRequest(format!(
            "Surec onay beklemiyor (mevcut: {status})"
        )));
    }
    let note = req.note.unwrap_or_default().trim().to_string();
    if note.is_empty() {
        return Err(AppError::BadRequest("Red sebebi zorunlu".into()));
    }

    if !ctx.is_owner {
        if let Some(required_role) = load_approver_role(&state.db, &pid).await? {
            let member_role: Option<(String,)> = sqlx::query_as(
                "SELECT r.id FROM workspace_members m JOIN roles r ON r.id = m.role_id
                 WHERE m.id = ?1",
            )
            .bind(&ctx.member_id)
            .fetch_optional(&state.db)
            .await?;
            if member_role.map(|(r,)| r) != Some(required_role) {
                return Err(AppError::Forbidden(
                    "Bu sureci yalnizca tanimli onay rolu reddedebilir".into(),
                ));
            }
        }
    }

    let now = util::now();
    let mut tx = state.db.begin().await?;
    sqlx::query(
        "UPDATE process_instances SET status = 'failed', finished_at = ?1 WHERE id = ?2",
    )
    .bind(&now)
    .bind(&pid)
    .execute(&mut *tx)
    .await?;
    // Aktif attempt'i basarisiz isaretle (Faz 7 — failure_reason ile)
    sqlx::query(
        "UPDATE process_attempts SET failed_at = ?1, status = 'failed', failure_reason = ?2, completed_by = ?3
         WHERE id = (SELECT current_attempt_id FROM process_instances WHERE id = ?4)",
    )
    .bind(&now)
    .bind(&note)
    .bind(&user.0.id)
    .bind(&pid)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "UPDATE approvals SET decision = 'rejected', decided_by = ?1, decided_at = ?2, note = ?3
         WHERE process_instance_id = ?4 AND decision = 'pending'",
    )
    .bind(&user.0.id)
    .bind(&now)
    .bind(&note)
    .bind(&pid)
    .execute(&mut *tx)
    .await?;
    // failed terminal degildir (rework yolu acik) — instance completed kontrolu YAPMAZ
    tx.commit().await?;
    crate::audit::audit(&state.db, &wid, Some(&user.0.id), "process.rejected", "process_instance", &pid,
        serde_json::json!({ "reason": note })).await;
    // Bildirim: atananlara (Faz 9)
    {
        let (pname, iname) = process_context(&state.db, &pid).await;
        let targets = crate::notifications::process_assignee_users(&state.db, &pid).await;
        crate::notifications::notify_users(
            &state.db, &wid, &targets, "process.rejected",
            &format!("Reddedildi: {pname}"),
            &format!("\"{iname}\" iş kalemindeki \"{pname}\" adımı reddedildi: {note}"),
            "process_instance", &pid,
        ).await;
    }
    Ok(Json(serde_json::json!({ "ok": true })))
}

// ---------------------------------------------------------------------------
// Surec atamalari (Faz 6 — yol haritasi 28)
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct AssignProcessReq {
    assignee_type: String, // user | team
    assignee_id: String,
}

/// Surece kullanici veya takim ata. Atama gecmisi silinmez (unassigned_at ile sonlanir).
pub async fn assign_process(
    State(state): State<AppState>,
    user: AuthUser,
    Path((wid, pid)): Path<(String, String)>,
    Json(req): Json<AssignProcessReq>,
) -> AppResult<(axum::http::StatusCode, Json<serde_json::Value>)> {
    let ctx = WsCtx::load(&state.db, &user, &wid).await?;
    ctx.require("process.assign")?;
    let (_, _) = load_process_checked(&state.db, &ctx, &wid, &pid).await?;

    if !matches!(req.assignee_type.as_str(), "user" | "team") {
        return Err(AppError::BadRequest("Atama tipi user veya team olmali".into()));
    }
    // Assignee gecerliligi
    if req.assignee_type == "user" {
        let ok: Option<(String,)> = sqlx::query_as(
            "SELECT id FROM workspace_members WHERE workspace_id = ?1 AND user_id = ?2 AND status = 'active' AND archived_at IS NULL",
        )
        .bind(&wid)
        .bind(&req.assignee_id)
        .fetch_optional(&state.db)
        .await?;
        if ok.is_none() {
            return Err(AppError::BadRequest("Kullanici bu workspace'in uyesi degil".into()));
        }
    } else {
        let ok: Option<(String,)> = sqlx::query_as(
            "SELECT id FROM teams WHERE id = ?1 AND workspace_id = ?2 AND archived_at IS NULL",
        )
        .bind(&req.assignee_id)
        .bind(&wid)
        .fetch_optional(&state.db)
        .await?;
        if ok.is_none() {
            return Err(AppError::BadRequest("Takim bulunamadi".into()));
        }
    }

    // Ayni atama aktif olarak var mi?
    let dup: Option<(String,)> = sqlx::query_as(
        "SELECT id FROM process_assignments
         WHERE process_instance_id = ?1 AND assignee_type = ?2 AND assignee_id = ?3 AND unassigned_at IS NULL",
    )
    .bind(&pid)
    .bind(&req.assignee_type)
    .bind(&req.assignee_id)
    .fetch_optional(&state.db)
    .await?;
    if dup.is_some() {
        return Err(AppError::Conflict("Bu atama zaten var".into()));
    }

    let id = util::new_id();
    sqlx::query(
        "INSERT INTO process_assignments (id, process_instance_id, assignee_type, assignee_id, assigned_by, assigned_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
    )
    .bind(&id)
    .bind(&pid)
    .bind(&req.assignee_type)
    .bind(&req.assignee_id)
    .bind(&user.0.id)
    .bind(util::now())
    .execute(&state.db)
    .await?;
    // Bildirim: atananlara (Faz 9)
    let (pname, iname) = process_context(&state.db, &pid).await;
    let targets = crate::notifications::process_assignee_users(&state.db, &pid).await;
    crate::notifications::notify_users(
        &state.db, &wid, &targets, "process.assigned",
        &format!("Yeni görev: {pname}"),
        &format!("\"{pname}\" adımı \"{iname}\" iş kaleminde size atandı."),
        "process_instance", &pid,
    ).await;

    Ok((axum::http::StatusCode::CREATED, Json(serde_json::json!({ "id": id }))))
}

/// Atamayi sonlandir (silmez — gecmis korunur).
pub async fn unassign_process(
    State(state): State<AppState>,
    user: AuthUser,
    Path((wid, pid, aid)): Path<(String, String, String)>,
) -> AppResult<Json<serde_json::Value>> {
    let ctx = WsCtx::load(&state.db, &user, &wid).await?;
    ctx.require("process.assign")?;
    let (_, _) = load_process_checked(&state.db, &ctx, &wid, &pid).await?;

    let res = sqlx::query(
        "UPDATE process_assignments SET unassigned_at = ?1
         WHERE id = ?2 AND process_instance_id = ?3 AND unassigned_at IS NULL",
    )
    .bind(util::now())
    .bind(&aid)
    .bind(&pid)
    .execute(&state.db)
    .await?;
    if res.rows_affected() == 0 {
        return Err(AppError::NotFound("Atama bulunamadi".into()));
    }
    Ok(Json(serde_json::json!({ "ok": true })))
}

/// Belirtilen instance icinde: TUM predecessor'lari approved olan waiting
/// surecleri ready yapar (all_completed kurali - MVP dependency tipi).
async fn promote_ready_successors(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    instance_id: &str,
    now: &str,
) -> AppResult<()> {
    // waiting surecler + onaylanmamis predecessor sayilari
    let rows: Vec<(String, i64)> = sqlx::query_as(
        "SELECT p.id,
            (SELECT COUNT(*) FROM workflow_dependencies d
             WHERE d.successor_node_id = p.workflow_node_id
               AND d.version_id = (SELECT workflow_version_id FROM workflow_instances WHERE id = ?1)
               AND d.predecessor_node_id NOT IN (
                   SELECT p2.workflow_node_id FROM process_instances p2
                   WHERE p2.workflow_instance_id = ?1 AND p2.status = 'approved'
               )) AS unapproved_preds
         FROM process_instances p
         WHERE p.workflow_instance_id = ?1 AND p.status = 'waiting'",
    )
    .bind(instance_id)
    .fetch_all(&mut **tx)
    .await?;

    for (pid, unapproved) in rows {
        if unapproved == 0 {
            sqlx::query(
                "UPDATE process_instances SET status = 'ready', ready_at = ?1 WHERE id = ?2",
            )
            .bind(now)
            .bind(&pid)
            .execute(&mut **tx)
            .await?;
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Iptal
// ---------------------------------------------------------------------------

/// Akis atamasini iptal eder: instance + aktif (terminal olmayan) surecler cancelled.
pub async fn cancel_assignment(
    State(state): State<AppState>,
    user: AuthUser,
    Path((wid, iid)): Path<(String, String)>,
) -> AppResult<Json<serde_json::Value>> {
    let ctx = WsCtx::load(&state.db, &user, &wid).await?;
    ctx.require("workflow.assign")?;
    check_item_scope(&state.db, &ctx, &wid, &iid).await?;

    let active: Option<(String,)> = sqlx::query_as(
        "SELECT id FROM workflow_instances WHERE work_item_id = ?1 AND status = 'active'",
    )
    .bind(&iid)
    .fetch_optional(&state.db)
    .await?;
    let (instance_id,) =
        active.ok_or_else(|| AppError::NotFound("Aktif akis atamasi yok".into()))?;

    let now = util::now();
    let mut tx = state.db.begin().await?;
    sqlx::query(
        "UPDATE process_instances SET status = 'cancelled', finished_at = ?1
         WHERE workflow_instance_id = ?2 AND status NOT IN ('approved', 'cancelled', 'skipped')",
    )
    .bind(&now)
    .bind(&instance_id)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "UPDATE workflow_instances SET status = 'cancelled', completed_at = ?1 WHERE id = ?2",
    )
    .bind(&now)
    .bind(&instance_id)
    .execute(&mut *tx)
    .await?;
    // Akış iptal -> kalem akış bekler (aktifse taslağa döner)
    sync_item_status(&mut tx, &iid, "cancelled", &now).await?;
    tx.commit().await?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

// ---------------------------------------------------------------------------
// Rework (Faz 7 — yol haritasi 29-31)
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct ReworkReq {
    restart_from_process_id: String,
    reason: String,
}

/// Rework baslatir: basarisiz sureci geriye CEVIRMEZ; restart noktasindan
/// yeni attempt zinciri olusturur. Upstream (approved) adimlar DOKUNULMAZ.
pub async fn start_rework(
    State(state): State<AppState>,
    user: AuthUser,
    Path((wid, iid)): Path<(String, String)>,
    Json(req): Json<ReworkReq>,
) -> AppResult<Json<serde_json::Value>> {
    let ctx = WsCtx::load(&state.db, &user, &wid).await?;
    ctx.require("process.rework")?;
    check_item_scope(&state.db, &ctx, &wid, &iid).await?;

    let reason = req.reason.trim().to_string();
    if reason.is_empty() {
        return Err(AppError::BadRequest("Rework sebebi zorunlu".into()));
    }

    // Instance + surecler
    let instance: Option<(String,)> = sqlx::query_as(
        "SELECT id FROM workflow_instances WHERE work_item_id = ?1 AND status IN ('active', 'completed') ORDER BY created_at DESC LIMIT 1",
    )
    .bind(&iid)
    .fetch_optional(&state.db)
    .await?;
    let (instance_id,) = instance
        .ok_or_else(|| AppError::NotFound("Aktif akis atamasi yok".into()))?;

    // Surecler: (id, node_id, status)
    let processes: Vec<(String, String, String)> = sqlx::query_as(
        "SELECT p.id, p.workflow_node_id, p.status FROM process_instances p
         WHERE p.workflow_instance_id = ?1",
    )
    .bind(&instance_id)
    .fetch_all(&state.db)
    .await?;

    // Failed sureci bul (trigger)
    let failed = processes
        .iter()
        .find(|(_, _, s)| s == "failed")
        .ok_or_else(|| AppError::BadRequest("Basarisiz surec yok; rework gereksiz".into()))?;

    // Restart sureci bu instance'a ait mi?
    let restart = processes
        .iter()
        .find(|(id, _, _)| *id == req.restart_from_process_id)
        .ok_or_else(|| AppError::BadRequest("Restart adimi bu akisa ait degil".into()))?;

    // Bagimliliklardan successor graph kur
    let deps: Vec<(String, String)> = sqlx::query_as(
        "SELECT d.predecessor_node_id, d.successor_node_id
         FROM workflow_dependencies d
         WHERE d.version_id = (SELECT workflow_version_id FROM workflow_instances WHERE id = ?1)",
    )
    .bind(&instance_id)
    .fetch_all(&state.db)
    .await?;
    let mut adj: std::collections::HashMap<String, Vec<String>> = std::collections::HashMap::new();
    for (p, s) in &deps {
        adj.entry(p.clone()).or_default().push(s.clone());
    }

    // Validasyon: failed node, restart node'dan reachable olmali (restart upstream'de)
    fn reaches_node(
        adj: &std::collections::HashMap<String, Vec<String>>,
        from: &str,
        target: &str,
    ) -> bool {
        let mut seen = std::collections::HashSet::new();
        let mut q = std::collections::VecDeque::new();
        q.push_back(from.to_string());
        seen.insert(from.to_string());
        while let Some(cur) = q.pop_front() {
            if cur == target {
                return true;
            }
            if let Some(nexts) = adj.get(&cur) {
                for n in nexts {
                    if seen.insert(n.clone()) {
                        q.push_back(n.clone());
                    }
                }
            }
        }
        false
    }
    if !reaches_node(&adj, &restart.1, &failed.1) {
        return Err(AppError::BadRequest(
            "Restart adimi, basarisiz surece giden yolda olmali".into(),
        ));
    }

    // Etkilenen surecler: restart + reachable successor'lari (node id bazli)
    let mut affected_node_ids = std::collections::HashSet::new();
    let mut q = std::collections::VecDeque::new();
    q.push_back(restart.1.clone());
    affected_node_ids.insert(restart.1.clone());
    while let Some(cur) = q.pop_front() {
        if let Some(nexts) = adj.get(&cur) {
            for n in nexts {
                if affected_node_ids.insert(n.clone()) {
                    q.push_back(n.clone());
                }
            }
        }
    }
    let affected: Vec<&(String, String, String)> = processes
        .iter()
        .filter(|(_, node_id, _)| affected_node_ids.contains(node_id))
        .collect();

    let now = util::now();
    let mut tx = state.db.begin().await?;

    // Rework cycle kaydi (yol haritasi 31)
    sqlx::query(
        "INSERT INTO rework_cycles (id, workflow_instance_id, triggered_by_process_id, restart_from_process_id, reason, created_by, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
    )
    .bind(util::new_id())
    .bind(&instance_id)
    .bind(&failed.0)
    .bind(&restart.0)
    .bind(&reason)
    .bind(&user.0.id)
    .bind(&now)
    .execute(&mut *tx)
    .await?;

    for (pid, node_id, _) in &affected {
        // Eski aktif attempt'leri superseded yap (gecmish korunur - asla silinmez)
        sqlx::query(
            "UPDATE process_attempts SET status = 'superseded'
             WHERE process_instance_id = ?1 AND status IN ('in_progress', 'submitted', 'failed')",
        )
        .bind(pid)
        .execute(&mut *tx)
        .await?;

        // Yeni attempt: restart -> ready, downstream -> waiting
        let is_restart = *node_id == restart.1;
        let max: Option<(i64,)> = sqlx::query_as(
            "SELECT MAX(attempt_number) FROM process_attempts WHERE process_instance_id = ?1",
        )
        .bind(pid)
        .fetch_optional(&mut *tx)
        .await?;
        let next = max.map(|(v,)| v).unwrap_or(0) + 1;
        let new_status = if is_restart { "ready" } else { "waiting" };

        let att_id = util::new_id();
        sqlx::query(
            "INSERT INTO process_attempts (id, process_instance_id, attempt_number, status, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
        )
        .bind(&att_id)
        .bind(pid)
        .bind(next)
        .bind(new_status)
        .bind(&now)
        .execute(&mut *tx)
        .await?;

        sqlx::query(
            "UPDATE process_instances SET status = ?1, finished_at = NULL, ready_at = CASE WHEN ?1 = 'ready' THEN ?2 ELSE ready_at END, current_attempt_id = ?3 WHERE id = ?4",
        )
        .bind(new_status)
        .bind(&now)
        .bind(&att_id)
        .bind(pid)
        .execute(&mut *tx)
        .await?;
    }

    // Instance yeniden aktif
    sqlx::query(
        "UPDATE workflow_instances SET status = 'active', completed_at = NULL WHERE id = ?1",
    )
    .bind(&instance_id)
    .execute(&mut *tx)
    .await?;
    // Rework -> kalem yeniden üretimde (aktif)
    sync_item_status(&mut tx, &iid, "assigned", &now).await?;

    tx.commit().await?;
    crate::audit::audit(&state.db, &wid, Some(&user.0.id), "rework.started", "work_item", &iid,
        serde_json::json!({ "reason": reason, "restart_process": req.restart_from_process_id, "affected": affected.len() })).await;
    // Bildirim: etkilenen sureclerin atananlarina (Faz 9)
    {
        let mut targets: Vec<String> = Vec::new();
        for (pid, _, _) in &affected {
            targets.extend(crate::notifications::process_assignee_users(&state.db, pid).await);
        }
        targets.dedup();
        let (rname, iname) = process_context(&state.db, &restart.0).await;
        crate::notifications::notify_users(
            &state.db, &wid, &targets, "rework.started",
            &format!("Rework başlatıldı: {iname}"),
            &format!("\"{iname}\" iş kaleminde \"{rname}\" adımından yeniden üretim başlatıldı: {reason}"),
            "work_item", &iid,
        ).await;
    }
    Ok(Json(serde_json::json!({
        "ok": true,
        "affected": affected.len()
    })))
}

/// Bildirim metni icin: surec adi + is kalemi adi.
async fn process_context(db: &sqlx::SqlitePool, pid: &str) -> (String, String) {
    sqlx::query_as(
        "SELECT n.name, w.name FROM process_instances p
         JOIN workflow_nodes n ON n.id = p.workflow_node_id
         JOIN workflow_instances wi ON wi.id = p.workflow_instance_id
         JOIN work_items w ON w.id = wi.work_item_id
         WHERE p.id = ?1",
    )
    .bind(pid)
    .fetch_optional(db)
    .await
    .ok()
    .flatten()
    .unwrap_or_else(|| ("?".to_string(), "?".to_string()))
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/workspaces/{wid}/work-items/{iid}/workflow",
            routing::get(get_assignment).post(assign).delete(cancel_assignment),
        )
        .route(
            "/workspaces/{wid}/work-items/{iid}/rework",
            routing::post(start_rework),
        )
        .route(
            "/workspaces/{wid}/process-instances/{pid}/start",
            routing::post(start_process),
        )
        .route(
            "/workspaces/{wid}/process-instances/{pid}/submit",
            routing::post(submit_process),
        )
        .route(
            "/workspaces/{wid}/process-instances/{pid}/approve",
            routing::post(approve_process),
        )
        .route(
            "/workspaces/{wid}/process-instances/{pid}/reject",
            routing::post(reject_process),
        )
        .route(
            "/workspaces/{wid}/process-instances/{pid}/assignments",
            routing::post(assign_process),
        )
        .route(
            "/workspaces/{wid}/process-instances/{pid}/assignments/{aid}",
            routing::delete(unassign_process),
        )
}
