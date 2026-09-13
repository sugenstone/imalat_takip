//! Faz 10 testleri: dashboard metrikleri (scope duyarli, rework oranlari, cycle time).

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use axum::Router;
use serde_json::{json, Value};
use tower::ServiceExt;

use imtk_backend::{build_router, test_state, state::AppState};

struct TestApp {
    app: Router,
    #[allow(dead_code)]
    state: AppState,
}

async fn setup() -> TestApp {
    let state = test_state().await;
    let app = build_router(state.clone());
    TestApp { app, state }
}

struct Client {
    cookie: Option<String>,
}

impl Client {
    fn new() -> Self {
        Client { cookie: None }
    }

    async fn send(
        &mut self,
        app: &Router,
        method: &str,
        uri: &str,
        body: Option<Value>,
    ) -> (StatusCode, Value) {
        let mut builder = Request::builder().method(method).uri(uri);
        if let Some(c) = &self.cookie {
            builder = builder.header(header::COOKIE, c);
        }
        let req = match body {
            Some(v) => builder
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(v.to_string()))
                .unwrap(),
            None => builder.body(Body::empty()).unwrap(),
        };
        let res = app.clone().oneshot(req).await.unwrap();
        if let Some(sc) = res.headers().get(header::SET_COOKIE) {
            let sc = sc.to_str().unwrap().to_string();
            let pair = sc.split(';').next().unwrap().to_string();
            if pair.starts_with("imtk_session=") && pair != "imtk_session=" {
                self.cookie = Some(pair);
            }
        }
        let status = res.status();
        let bytes = axum::body::to_bytes(res.into_body(), 1024 * 1024).await.unwrap();
        let body: Value = if bytes.is_empty() {
            Value::Null
        } else {
            serde_json::from_slice(&bytes).unwrap_or(Value::Null)
        };
        (status, body)
    }
}

#[tokio::test]
async fn dashboard_metrics_with_rework() {
    let app = setup().await;
    let mut ali = Client::new();
    ali.send(&app.app, "POST", "/api/auth/register",
        Some(json!({ "email": "ali@t.com", "password": "parola123", "name": "Ali" }))).await;
    let mut veli = Client::new();
    veli.send(&app.app, "POST", "/api/auth/register",
        Some(json!({ "email": "veli@t.com", "password": "parola123", "name": "Veli" }))).await;

    let (_, ws) = ali.send(&app.app, "POST", "/api/workspaces",
        Some(json!({ "name": "Atolye" }))).await;
    let wid = ws["id"].as_str().unwrap().to_string();

    let (_, roles) = ali.send(&app.app, "GET", &format!("/api/workspaces/{wid}/roles"), None).await;
    let sup_id = roles.as_array().unwrap().iter()
        .find(|r| r["name"] == "Supervisor").map(|r| r["id"].as_str().unwrap().to_string()).unwrap();
    ali.send(&app.app, "POST", &format!("/api/workspaces/{wid}/members"),
        Some(json!({ "email": "veli@t.com", "role_id": sup_id }))).await;

    let (_, blok) = ali.send(&app.app, "POST", &format!("/api/workspaces/{wid}/sections"),
        Some(json!({ "name": "Blok" }))).await;

    // A -> B(onayli Supervisor) akisi
    let (_, wf) = ali.send(&app.app, "POST", &format!("/api/workspaces/{wid}/workflows"),
        Some(json!({ "name": "Akis" }))).await;
    let tfid = wf["id"].as_str().unwrap().to_string();
    let (_, na) = ali.send(&app.app, "POST",
        &format!("/api/workspaces/{wid}/workflows/{tfid}/draft/nodes"),
        Some(json!({ "name": "A" }))).await;
    let (_, nb) = ali.send(&app.app, "POST",
        &format!("/api/workspaces/{wid}/workflows/{tfid}/draft/nodes"),
        Some(json!({ "name": "B" }))).await;
    ali.send(&app.app, "PATCH",
        &format!("/api/workspaces/{wid}/workflows/{tfid}/draft/nodes/{}", nb["id"].as_str().unwrap()),
        Some(json!({ "approval_rule": { "required": true, "approver_role_id": sup_id } }))).await;
    ali.send(&app.app, "POST",
        &format!("/api/workspaces/{wid}/workflows/{tfid}/draft/dependencies"),
        Some(json!({ "predecessor_node_id": na["id"], "successor_node_id": nb["id"] }))).await;
    ali.send(&app.app, "POST", &format!("/api/workspaces/{wid}/workflows/{tfid}/publish"), None).await;

    // Is kalemi 1: geciken (planned_end gecmis, active)
    let (_, item1) = ali.send(&app.app, "POST", &format!("/api/workspaces/{wid}/work-items"),
        Some(json!({ "section_id": blok["id"], "name": "Geciken", "status": "active",
                     "planned_end": "2020-01-01" }))).await;
    let iid1 = item1["id"].as_str().unwrap().to_string();

    // Is kalemi 2: rework'lu tamamlanan
    let (_, item2) = ali.send(&app.app, "POST", &format!("/api/workspaces/{wid}/work-items"),
        Some(json!({ "section_id": blok["id"], "name": "Reworklu", "status": "active" }))).await;
    let iid2 = item2["id"].as_str().unwrap().to_string();

    for iid in [&iid1, &iid2] {
        ali.send(&app.app, "POST", &format!("/api/workspaces/{wid}/work-items/{iid}/workflow"),
            Some(json!({ "template_id": tfid }))).await;
    }

    // iid1: A basla (in_progress birak) -> devam eden surec
    let (_, i1) = ali.send(&app.app, "GET",
        &format!("/api/workspaces/{wid}/work-items/{iid1}/workflow"), None).await;
    let a1 = i1["processes"].as_array().unwrap().iter()
        .find(|p| p["name"] == "A").map(|p| p["id"].as_str().unwrap().to_string()).unwrap();
    ali.send(&app.app, "POST", &format!("/api/workspaces/{wid}/process-instances/{a1}/start"),
        Some(json!({}))).await;

    // iid2: A tamamla, B basla+submit, REDDET, rework A'dan, yeniden tamamla
    let (_, i2w) = ali.send(&app.app, "GET",
        &format!("/api/workspaces/{wid}/work-items/{iid2}/workflow"), None).await;
    let a2 = i2w["processes"].as_array().unwrap().iter()
        .find(|p| p["name"] == "A").map(|p| p["id"].as_str().unwrap().to_string()).unwrap();
    let b2 = i2w["processes"].as_array().unwrap().iter()
        .find(|p| p["name"] == "B").map(|p| p["id"].as_str().unwrap().to_string()).unwrap();
    for pid in [&a2] {
        ali.send(&app.app, "POST", &format!("/api/workspaces/{wid}/process-instances/{pid}/start"),
            Some(json!({}))).await;
        ali.send(&app.app, "POST", &format!("/api/workspaces/{wid}/process-instances/{pid}/submit"),
            Some(json!({}))).await;
    }
    ali.send(&app.app, "POST", &format!("/api/workspaces/{wid}/process-instances/{b2}/start"),
        Some(json!({}))).await;
    ali.send(&app.app, "POST", &format!("/api/workspaces/{wid}/process-instances/{b2}/submit"),
        Some(json!({}))).await;
    veli.send(&app.app, "POST", &format!("/api/workspaces/{wid}/process-instances/{b2}/reject"),
        Some(json!({ "note": "hata" }))).await;

    // Dashboard (rework ONCESI): failed=1, submitted=0
    let (_, d1) = ali.send(&app.app, "GET", &format!("/api/workspaces/{wid}/reports/dashboard"), None).await;
    assert_eq!(d1["active_items"], 2);
    assert_eq!(d1["overdue_items"], 1, "planned_end gecmis olmali");
    assert_eq!(d1["in_progress_processes"], 1);
    assert_eq!(d1["failed_processes"], 1);
    assert_eq!(d1["rework_count"], 0);
    assert_eq!(d1["completed_items"], 0);
    assert!(d1["avg_cycle_time_hours"].is_null(), "henutz tamamlanan yok");

    // Rework + tamamla
    ali.send(&app.app, "POST", &format!("/api/workspaces/{wid}/work-items/{iid2}/rework"),
        Some(json!({ "restart_from_process_id": a2, "reason": "tekrar" }))).await;
    for pid in [&a2, &b2] {
        ali.send(&app.app, "POST", &format!("/api/workspaces/{wid}/process-instances/{pid}/start"),
            Some(json!({}))).await;
        ali.send(&app.app, "POST", &format!("/api/workspaces/{wid}/process-instances/{pid}/submit"),
            Some(json!({}))).await;
    }
    veli.send(&app.app, "POST", &format!("/api/workspaces/{wid}/process-instances/{b2}/approve"),
        Some(json!({}))).await;

    // Is kalemi 2 status -> completed yap
    ali.send(&app.app, "PATCH", &format!("/api/workspaces/{wid}/work-items/{iid2}"),
        Some(json!({ "name": "Reworklu", "status": "completed" }))).await;

    // Dashboard (rework SONRASI)
    let (st, d2) = ali.send(&app.app, "GET", &format!("/api/workspaces/{wid}/reports/dashboard"), None).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(d2["rework_count"], 1);
    assert_eq!(d2["completed_items"], 1);
    assert_eq!(d2["active_items"], 1);
    // 1 tamamlanan instance, o da rework'lu -> ilk seferde basari %0
    assert_eq!(d2["first_pass_success_rate"], 0, "tek tamamlanan rework'lu: {d2:?}");
    assert_eq!(d2["rework_rate"], 50, "2 instance'tan 1'i rework'lu");
    // cycle time artik sayisal
    assert!(d2["avg_cycle_time_hours"].is_number(), "cycle time hesaplanmali");
    // En cok hata: B (1 failed attempt)
    let top = d2["top_failed_processes"].as_array().unwrap();
    assert_eq!(top[0]["name"], "B");
    assert_eq!(top[0]["cnt"], 1);
    // Surec sureleri: A ve B approved, ortalama saniyeler dolu
    let durs = d2["process_durations"].as_array().unwrap();
    assert!(!durs.is_empty(), "süre süreleri dolmali: {d2:?}");

    // --- SCOPE: veli'yi baska bolume kilitle -> metrikler sifirlanmali ---
    let (_, blok2) = ali.send(&app.app, "POST", &format!("/api/workspaces/{wid}/sections"),
        Some(json!({ "name": "Blok2" }))).await;
    let (_, members) = ali.send(&app.app, "GET", &format!("/api/workspaces/{wid}/members"), None).await;
    let veli_mid = members.as_array().unwrap().iter()
        .find(|m| m["email"] == "veli@t.com").map(|m| m["id"].as_str().unwrap().to_string()).unwrap();
    ali.send(&app.app, "PUT", &format!("/api/workspaces/{wid}/members/{veli_mid}/scopes"),
        Some(json!({ "section_ids": [blok2["id"]] }))).await;

    let (_, d3) = veli.send(&app.app, "GET", &format!("/api/workspaces/{wid}/reports/dashboard"), None).await;
    assert_eq!(d3["active_items"], 0, "kapsam disi metrik gorunmemeli");
    assert_eq!(d3["rework_count"], 0);
    assert_eq!(d3["completed_items"], 0);
}
