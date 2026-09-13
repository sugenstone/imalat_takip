//! Faz 10: Dashboard + raporlar (yol haritasi 47).
//! Tum metrikler scope'a duyarli: kapsamli uye yalniz kendi subtree'sinden sayar.

use axum::extract::{Path, State};
use axum::{routing, Json, Router};
use serde::Serialize;

use crate::auth::AuthUser;
use crate::error::AppResult;
use crate::state::AppState;
use crate::util;
use crate::ws_ctx::WsCtx;

#[derive(Serialize, sqlx::FromRow)]
pub struct NameCount {
    pub name: String,
    pub cnt: i64,
}

#[derive(Serialize, sqlx::FromRow)]
pub struct NameAvg {
    pub name: String,
    pub avg_seconds: f64,
}

#[derive(Serialize)]
pub struct DashboardOut {
    // Is kalemleri
    pub active_items: i64,
    pub completed_items: i64,
    pub overdue_items: i64,
    // Surecler (aktif instance'lardaki)
    pub ready_processes: i64,
    pub in_progress_processes: i64,
    pub pending_approval_processes: i64,
    pub failed_processes: i64,
    // Rework
    pub rework_count: i64,
    // Uye sayisi
    pub member_count: i64,
    // Ileri raporlar
    pub avg_cycle_time_hours: Option<f64>,
    pub first_pass_success_rate: Option<i64>,
    pub rework_rate: Option<i64>,
    pub top_failed_processes: Vec<NameCount>,
    pub process_durations: Vec<NameAvg>,
}

/// Kapsam filtresi: uyenin scope'u varsa is kalemi section path Kosulu dondurur.
/// (None = tum workspace)
fn scope_condition(ctx: &WsCtx) -> Option<String> {
    ctx.scope_paths.as_ref().map(|paths| {
        let ors: Vec<String> = paths
            .iter()
            .map(|p| format!("s.path_cache = '{p}' OR s.path_cache LIKE '{p}/%'"))
            .collect();
        format!("({})", ors.join(" OR "))
    })
}

async fn count_sql(db: &sqlx::SqlitePool, sql: &str, wid: &str) -> i64 {
    sqlx::query_as::<_, (i64,)>(sql)
        .bind(wid)
        .fetch_one(db)
        .await
        .map(|(c,)| c)
        .unwrap_or(0)
}

async fn count_proc(db: &sqlx::SqlitePool, proc_filter: &str, wid: &str, status: &str) -> i64 {
    let sql = format!(
        "SELECT COUNT(*) FROM process_instances p JOIN workflow_instances wi ON wi.id = p.workflow_instance_id {proc_filter} AND p.status = ?2"
    );
    sqlx::query_as::<_, (i64,)>(&sql)
        .bind(wid)
        .bind(status)
        .fetch_one(db)
        .await
        .map(|(c,)| c)
        .unwrap_or(0)
}

pub async fn dashboard(
    State(state): State<AppState>,
    user: AuthUser,
    Path(wid): Path<String>,
) -> AppResult<Json<DashboardOut>> {
    let ctx = WsCtx::load(&state.db, &user, &wid).await?;
    let scope = scope_condition(&ctx);
    let now = util::now();

    // Is kalemi sayilari (scope'lu)
    let (item_sql_scope, item_join) = match &scope {
        Some(cond) => (
            format!(" AND w.archived_at IS NULL AND {cond}"),
            "JOIN sections s ON s.id = w.section_id".to_string(),
        ),
        None => (" AND w.archived_at IS NULL".to_string(), String::new()),
    };

    let active_items = count_sql(&state.db, &format!(
        "SELECT COUNT(*) FROM work_items w {item_join} WHERE w.workspace_id = ?1 AND w.status = 'active'{item_sql_scope}"
    ), &wid).await;
    let completed_items = count_sql(&state.db, &format!(
        "SELECT COUNT(*) FROM work_items w {item_join} WHERE w.workspace_id = ?1 AND w.status = 'completed'{item_sql_scope}"
    ), &wid).await;
    let overdue_items = count_sql(&state.db, &format!(
        "SELECT COUNT(*) FROM work_items w {item_join}
         WHERE w.workspace_id = ?1 AND w.status IN ('active','blocked')
           AND w.planned_end IS NOT NULL AND w.planned_end < '{now}'{item_sql_scope}"
    ), &wid).await;

    // Surecler: scope'lu is kalemlerinin AKTIF instance'larindaki
    let proc_filter = match &scope {
        Some(cond) => format!(
            "JOIN work_items w2 ON w2.id = wi.work_item_id JOIN sections s ON s.id = w2.section_id
             WHERE wi.work_item_id IN (SELECT w.id FROM work_items w JOIN sections s ON s.id = w.section_id WHERE w.workspace_id = ?1 AND w.archived_at IS NULL AND {cond})
               AND wi.status = 'active'"
        ),
        None => "WHERE wi.work_item_id IN (SELECT id FROM work_items WHERE workspace_id = ?1) AND wi.status = 'active'".to_string(),
    };

    let ready_processes = count_proc(&state.db, &proc_filter, &wid, "ready").await;
    let in_progress_processes = count_proc(&state.db, &proc_filter, &wid, "in_progress").await;
    let pending_approval_processes = count_proc(&state.db, &proc_filter, &wid, "submitted").await;
    let failed_processes = count_proc(&state.db, &proc_filter, &wid, "failed").await;

    // Rework sayisi (scope'lu is kalemlerinde)
    let member_count = count_sql(
        &state.db,
        "SELECT COUNT(*) FROM workspace_members WHERE workspace_id = ?1 AND status = 'active' AND archived_at IS NULL",
        &wid,
    )
    .await;

    let rework_count = {
        let sql = match &scope {
            Some(cond) => format!(
                "SELECT COUNT(*) FROM rework_cycles rc JOIN workflow_instances wi ON wi.id = rc.workflow_instance_id
                 JOIN work_items w ON w.id = wi.work_item_id JOIN sections s ON s.id = w.section_id
                 WHERE w.workspace_id = ?1 AND {cond}"
            ),
            None => "SELECT COUNT(*) FROM rework_cycles rc JOIN workflow_instances wi ON wi.id = rc.workflow_instance_id
                     JOIN work_items w ON w.id = wi.work_item_id WHERE w.workspace_id = ?1".to_string(),
        };
        sqlx::query_as::<_, (i64,)>(&sql)
            .bind(&wid)
            .fetch_one(&state.db)
            .await
            .map(|(c,)| c)
            .unwrap_or(0)
    };

    // Cycle time + basari oranlari (scope'lu)
    let inst_where = match &scope {
        Some(cond) => format!(
            "wi.work_item_id IN (SELECT w.id FROM work_items w JOIN sections s ON s.id = w.section_id WHERE w.workspace_id = ?1 AND {cond})"
        ),
        None => "wi.work_item_id IN (SELECT id FROM work_items WHERE workspace_id = ?1)".to_string(),
    };

    let completed: Option<(i64, Option<f64>)> = sqlx::query_as(&format!(
        "SELECT COUNT(*), AVG((julianday(wi.completed_at) - julianday(wi.started_at)) * 24.0)
         FROM workflow_instances wi WHERE wi.status = 'completed' AND {inst_where}"
    ))
    .bind(&wid)
    .fetch_optional(&state.db)
    .await?;

    let reworked: i64 = sqlx::query_as(&format!(
        "SELECT COUNT(DISTINCT wi.id) FROM workflow_instances wi
         JOIN rework_cycles rc ON rc.workflow_instance_id = wi.id WHERE {inst_where}"
    ))
    .bind(&wid)
    .fetch_one(&state.db)
    .await
    .map(|(c,)| c)
    .unwrap_or(0);

    let total_instances: i64 = sqlx::query_as(&format!(
        "SELECT COUNT(*) FROM workflow_instances wi WHERE {inst_where}"
    ))
    .bind(&wid)
    .fetch_one(&state.db)
    .await
    .map(|(c,)| c)
    .unwrap_or(0);

    let (completed_count, avg_hours) = completed.unwrap_or((0, None));
    let first_pass_rate = if completed_count > 0 {
        let fp = completed_count - reworked.min(completed_count);
        Some(fp * 100 / completed_count)
    } else {
        None
    };
    let rework_rate = if total_instances > 0 {
        Some(reworked * 100 / total_instances)
    } else {
        None
    };

    // En cok hata cikan surecler (failed attempt gecmisi, node bazinda).
    // Not: rework'ta eski denemeler superseded olsa da failed_at damgasi kalici.
    let top_failed_join = match &scope {
        Some(cond) => format!(
            "JOIN workflow_instances wi ON wi.id = p.workflow_instance_id
             JOIN work_items w ON w.id = wi.work_item_id JOIN sections s ON s.id = w.section_id
             WHERE w.workspace_id = ?1 AND {cond}"
        ),
        None => "JOIN workflow_instances wi ON wi.id = p.workflow_instance_id
                 JOIN work_items w ON w.id = wi.work_item_id WHERE w.workspace_id = ?1".to_string(),
    };
    let failed_attempt_cond = " AND pa.failed_at IS NOT NULL";
    let top_failed_processes = sqlx::query_as::<_, NameCount>(&format!(
        "SELECT n.name, COUNT(*) AS cnt
         FROM process_attempts pa
         JOIN process_instances p ON p.id = pa.process_instance_id
         JOIN workflow_nodes n ON n.id = p.workflow_node_id
         {top_failed_join}{failed_attempt_cond}
         GROUP BY n.name ORDER BY cnt DESC LIMIT 5"
    ))
    .bind(&wid)
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();

    // Surec bazinda ortalama sure (approved surecler)
    let durations_where = match &scope {
        Some(cond) => format!("WHERE w.workspace_id = ?1 AND {cond}"),
        None => "WHERE w.workspace_id = ?1".to_string(),
    };
    let process_durations = sqlx::query_as::<_, NameAvg>(&format!(
        "SELECT n.name, AVG((julianday(p.finished_at) - julianday(p.started_at)) * 86400.0) AS avg_seconds
         FROM process_instances p
         JOIN workflow_nodes n ON n.id = p.workflow_node_id
         JOIN workflow_instances wi ON wi.id = p.workflow_instance_id
         JOIN work_items w ON w.id = wi.work_item_id
         {durations_where}
           AND p.status = 'approved' AND p.started_at IS NOT NULL AND p.finished_at IS NOT NULL
         GROUP BY n.name ORDER BY avg_seconds DESC LIMIT 10"
    ))
    .bind(&wid)
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();

    Ok(Json(DashboardOut {
        active_items,
        completed_items,
        overdue_items,
        ready_processes,
        in_progress_processes,
        pending_approval_processes,
        failed_processes,
        rework_count,
        member_count,
        avg_cycle_time_hours: avg_hours,
        first_pass_success_rate: first_pass_rate,
        rework_rate,
        top_failed_processes,
        process_durations,
    }))
}

pub fn router() -> Router<AppState> {
    Router::new().route("/workspaces/{wid}/reports/dashboard", routing::get(dashboard))
}
