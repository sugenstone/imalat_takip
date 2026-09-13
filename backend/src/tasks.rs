//! "Gorevlerim" ekrani verisi: bana atanan surecler + onaylayabilecegim surecler.
//! Aksiyonlar (start/submit/approve/reject) mevcut endpoint'leri kullanir.

use axum::extract::{Path, State};
use axum::{routing, Json, Router};
use serde::Serialize;

use crate::auth::AuthUser;
use crate::error::AppResult;
use crate::state::AppState;
use crate::ws_ctx::WsCtx;

#[derive(Serialize, sqlx::FromRow)]
#[derive(Clone)]
pub struct TaskRow {
    pub id: String,
    pub step_name: String,
    pub status: String,
    pub item_id: String,
    pub item_name: String,
    pub item_status: String,
    pub workflow_name: String,
    pub assignments: Option<String>, // JSON array
    pub approval_note: Option<String>,
    pub requested_by_name: Option<String>,
}

#[derive(Serialize)]
pub struct MyTasksOut {
    pub ready: Vec<TaskRow>,
    pub in_progress: Vec<TaskRow>,
    pub approvals: Vec<TaskRow>,
}

const TASK_SELECT: &str = "SELECT p.id,
    n.name AS step_name,
    p.status,
    w.id AS item_id,
    w.name AS item_name,
    w.status AS item_status,
    t.name AS workflow_name,
    (SELECT json_group_array(json_object('type', pa.assignee_type, 'name',
        CASE WHEN pa.assignee_type = 'user' THEN (SELECT u.name FROM users u WHERE u.id = pa.assignee_id)
             ELSE (SELECT tm.name FROM teams tm WHERE tm.id = pa.assignee_id) END))
     FROM process_assignments pa WHERE pa.process_instance_id = p.id AND pa.unassigned_at IS NULL) AS assignments,
    (SELECT ap.note FROM approvals ap WHERE ap.process_instance_id = p.id AND ap.decision = 'pending' LIMIT 1) AS approval_note,
    (SELECT u2.name FROM approvals ap2 JOIN users u2 ON u2.id = ap2.requested_by WHERE ap2.process_instance_id = p.id AND ap2.decision = 'pending' LIMIT 1) AS requested_by_name
 FROM process_instances p
 JOIN workflow_instances wi ON wi.id = p.workflow_instance_id
 JOIN workflow_nodes n ON n.id = p.workflow_node_id
 JOIN workflow_versions v ON v.id = wi.workflow_version_id
 JOIN workflow_templates t ON t.id = v.template_id
 JOIN work_items w ON w.id = wi.work_item_id";

pub async fn my_tasks(
    State(state): State<AppState>,
    user: AuthUser,
    Path(wid): Path<String>,
) -> AppResult<Json<MyTasksOut>> {
    let ctx = WsCtx::load(&state.db, &user, &wid).await?;

    // 1) Bana atanan (user + takim) surecler — scope filtreli
    let scope_mine = ctx
        .scope_paths
        .as_ref()
        .map(|paths| {
            paths
                .iter()
                .map(|p| format!("s.path_cache = '{p}' OR s.path_cache LIKE '{p}/%'"))
                .collect::<Vec<_>>()
                .join(" OR ")
        })
        .unwrap_or_else(|| "1=1".into());
    let mine_sql = format!(
        "{TASK_SELECT}
         JOIN sections s ON s.id = w.section_id
         WHERE w.workspace_id = ?1 AND wi.status = 'active' AND p.status IN ('ready', 'in_progress')
           AND EXISTS (
             SELECT 1 FROM process_assignments pa
             WHERE pa.process_instance_id = p.id AND pa.unassigned_at IS NULL
               AND ((pa.assignee_type = 'user' AND pa.assignee_id = ?2)
                 OR (pa.assignee_type = 'team' AND pa.assignee_id IN
                     (SELECT team_id FROM team_members WHERE user_id = ?2)))
           )
           AND ({scope_mine})
         ORDER BY p.status DESC, w.name LIMIT 200"
    );
    let mine: Vec<TaskRow> = sqlx::query_as(mine_sql.as_str())
    .bind(&wid)
    .bind(&user.0.id)
    .fetch_all(&state.db)
    .await?;

    let ready: Vec<TaskRow> = mine.iter().filter(|t| t.status == "ready").cloned().collect();
    let in_progress: Vec<TaskRow> = mine
        .iter()
        .filter(|t| t.status == "in_progress")
        .cloned()
        .collect();

    // 2) Onaylayabilecegim surecler: submitted + (iznim var VE (kural rolu yok VEYA rolum uyusuyor VEYA owner))
    let mut approvals: Vec<TaskRow> = Vec::new();
    if ctx.permissions.contains("process.approve") {
        let scope_cond = ctx
            .scope_paths
            .as_ref()
            .map(|paths| {
                paths
                    .iter()
                    .map(|p| format!("s.path_cache = '{p}' OR s.path_cache LIKE '{p}/%'"))
                    .collect::<Vec<_>>()
                    .join(" OR ")
            })
            .unwrap_or_else(|| "1=1".into());
        let submitted_sql = format!(
            "SELECT p.id, n.approval_rule_json
             FROM process_instances p
             JOIN workflow_instances wi ON wi.id = p.workflow_instance_id
             JOIN workflow_nodes n ON n.id = p.workflow_node_id
             JOIN work_items w ON w.id = wi.work_item_id
             JOIN sections s ON s.id = w.section_id
             WHERE w.workspace_id = ?1 AND wi.status = 'active' AND p.status = 'submitted'
               AND ({scope_cond})
             LIMIT 300"
        );
        let submitted: Vec<(String, Option<String>)> = sqlx::query_as(submitted_sql.as_str())
        .bind(&wid)
        .fetch_all(&state.db)
        .await?;

        // Kullanici rol id'si
        let my_role: Option<String> = sqlx::query_as::<_, (String,)>(
            "SELECT r.id FROM workspace_members m JOIN roles r ON r.id = m.role_id WHERE m.id = ?1",
        )
        .bind(&ctx.member_id)
        .fetch_optional(&state.db)
        .await?
        .map(|(r,)| r);

        for (pid, rule) in submitted {
            let required_role = rule
                .and_then(|r| serde_json::from_str::<serde_json::Value>(&r).ok())
                .and_then(|v| {
                    v.get("approver_role_id")
                        .and_then(|x| x.as_str())
                        .map(|s| s.to_string())
                });
            let ok = match required_role {
                None => true, // kural rolu yok → izni olan herkes
                Some(rr) => ctx.is_owner || my_role.as_deref() == Some(rr.as_str()),
            };
            if ok {
                if let Ok(row) = sqlx::query_as::<_, TaskRow>(&format!(
                    "{TASK_SELECT} WHERE p.id = ?1"
                ))
                .bind(&pid)
                .fetch_one(&state.db)
                .await
                {
                    approvals.push(row);
                }
            }
        }
    }

    Ok(Json(MyTasksOut {
        ready,
        in_progress,
        approvals,
    }))
}

pub fn router() -> Router<AppState> {
    Router::new().route("/workspaces/{wid}/my-tasks", routing::get(my_tasks))
}
