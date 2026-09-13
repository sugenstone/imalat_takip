//! Akis -> is kalemi durum senkronu testleri:
//! assign -> active, tamamla -> completed, iptal -> draft, rework -> active.

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

/// Ali owner. Kalem + 2 adimli akis (A kuralsiz, B kuralsiz) kurulu.
async fn bootstrap() -> (TestApp, Client, String, String) {
    let app = setup().await;
    let mut ali = Client::new();
    ali.send(&app.app, "POST", "/api/auth/register",
        Some(json!({ "email": "ali@t.com", "password": "parola123", "name": "Ali" }))).await;

    let (_, ws) = ali.send(&app.app, "POST", "/api/workspaces",
        Some(json!({ "name": "Atolye" }))).await;
    let wid = ws["id"].as_str().unwrap().to_string();

    let (_, sec) = ali.send(&app.app, "POST", &format!("/api/workspaces/{wid}/sections"),
        Some(json!({ "name": "Blok" }))).await;
    let (_, item) = ali.send(&app.app, "POST", &format!("/api/workspaces/{wid}/work-items"),
        Some(json!({ "section_id": sec["id"], "name": "Kalem" }))).await;
    let iid = item["id"].as_str().unwrap().to_string();

    let (st, qf) = ali.send(&app.app, "POST", &format!("/api/workspaces/{wid}/quick-flow"),
        Some(json!({
            "name": "Akis",
            "steps": [ { "name": "A" }, { "name": "B" } ],
            "assign_to_item_id": iid
        }))).await;
    assert_eq!(st, StatusCode::OK, "quick-flow: {qf:?}");

    (app, ali, wid, iid)
}

async fn item_status(ali: &mut Client, app: &Router, wid: &str, iid: &str) -> String {
    let (st, item) = ali.send(app, "GET", &format!("/api/workspaces/{wid}/work-items/{iid}"), None).await;
    assert_eq!(st, StatusCode::OK);
    item["status"].as_str().unwrap().to_string()
}

async fn processes(ali: &mut Client, app: &Router, wid: &str, iid: &str) -> Vec<(String, String)> {
    let (_, inst) = ali.send(app, "GET", &format!("/api/workspaces/{wid}/work-items/{iid}/workflow"), None).await;
    inst["processes"].as_array().unwrap().iter()
        .map(|p| (p["id"].as_str().unwrap().to_string(), p["name"].as_str().unwrap().to_string()))
        .collect()
}

#[tokio::test]
async fn flow_lifecycle_syncs_item_status() {
    let (app, mut ali, wid, iid) = bootstrap().await;

    // 1) Kalem olusur -> draft; akis baglaninca -> active
    assert_eq!(item_status(&mut ali, &app.app, &wid, &iid).await, "active",
        "akis baglanan kalem aktif olmali");

    // 2) Tum adimlari bitir -> kalem completed
    let procs = processes(&mut ali, &app.app, &wid, &iid).await;
    assert_eq!(procs.len(), 2);
    for (pid, _) in &procs {
        ali.send(&app.app, "POST", &format!("/api/workspaces/{wid}/process-instances/{pid}/start"),
            Some(json!({}))).await;
        ali.send(&app.app, "POST", &format!("/api/workspaces/{wid}/process-instances/{pid}/submit"),
            Some(json!({}))).await;
    }
    assert_eq!(item_status(&mut ali, &app.app, &wid, &iid).await, "completed",
        "akisi biten kalem tamamlanmis olmali");

    // 3) Yeni akis daha ata -> yeniden active
    let (st, qf) = ali.send(&app.app, "POST", &format!("/api/workspaces/{wid}/quick-flow"),
        Some(json!({ "name": "Akis 2", "steps": [ { "name": "C" } ] }))).await;
    assert_eq!(st, StatusCode::OK, "quick-flow 2");
    let tid = qf["template_id"].as_str().unwrap().to_string();
    let (st, _) = ali.send(&app.app, "POST", &format!("/api/workspaces/{wid}/work-items/{iid}/workflow"),
        Some(json!({ "template_id": tid }))).await;
    assert_eq!(st, StatusCode::CREATED);
    assert_eq!(item_status(&mut ali, &app.app, &wid, &iid).await, "active",
        "yeniden akis baglanan kalem aktif olmali");

    // 4) Akisi iptal et -> kalem draft'a doner
    let (st, _) = ali.send(&app.app, "DELETE", &format!("/api/workspaces/{wid}/work-items/{iid}/workflow"), None).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(item_status(&mut ali, &app.app, &wid, &iid).await, "draft",
        "akisi iptal edilen kalem taslaga donmeli");

    // 5) Manuel 'cancelled' kalemin statusu akis tarafindan ezilmez
    let sec_id = section_of(&mut ali, &app.app, &wid).await;
    let (_, _) = ali.send(&app.app, "PATCH", &format!("/api/workspaces/{wid}/work-items/{iid}"),
        Some(json!({ "name": "Kalem", "status": "cancelled", "section_id": sec_id }))).await;
    let (st, _) = ali.send(&app.app, "POST", &format!("/api/workspaces/{wid}/work-items/{iid}/workflow"),
        Some(json!({ "template_id": tid }))).await;
    assert_eq!(st, StatusCode::CREATED);
    assert_eq!(item_status(&mut ali, &app.app, &wid, &iid).await, "cancelled",
        "iptal edilmis kalem statusu akis assign ile ezilmemeli");
}

/// Ilk bolumun id'si (PATCH section_id zorunlu ise).
async fn section_of(ali: &mut Client, app: &Router, wid: &str) -> String {
    let (_, secs) = ali.send(app, "GET", &format!("/api/workspaces/{wid}/sections"), None).await;
    secs.as_array().unwrap()[0]["id"].as_str().unwrap().to_string()
}

#[tokio::test]
async fn rework_keeps_item_active_after_reject() {
    let (app, mut ali, wid, _iid) = bootstrap().await;

    // Ikinci kalem + onayli akis: A -> B(requires_approval)
    let (_, secs) = ali.send(&app.app, "GET", &format!("/api/workspaces/{wid}/sections"), None).await;
    let sec_id = secs.as_array().unwrap()[0]["id"].as_str().unwrap().to_string();
    let (_, item) = ali.send(&app.app, "POST", &format!("/api/workspaces/{wid}/work-items"),
        Some(json!({ "section_id": sec_id, "name": "Kalem 2" }))).await;
    let iid = item["id"].as_str().unwrap().to_string();
    let (st, _) = ali.send(&app.app, "POST", &format!("/api/workspaces/{wid}/quick-flow"),
        Some(json!({
            "name": "Akis Onayli",
            "steps": [ { "name": "A" }, { "name": "B", "requires_approval": true } ],
            "assign_to_item_id": iid
        }))).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(item_status(&mut ali, &app.app, &wid, &iid).await, "active");

    // A'yi bitir; B'yi gonder ve REDDET -> B failed (instance rework icin acik kalir)
    let procs = processes(&mut ali, &app.app, &wid, &iid).await;
    let (a_id, _) = procs[0].clone();
    let (b_id, _) = procs[1].clone();
    ali.send(&app.app, "POST", &format!("/api/workspaces/{wid}/process-instances/{a_id}/start"),
        Some(json!({}))).await;
    ali.send(&app.app, "POST", &format!("/api/workspaces/{wid}/process-instances/{a_id}/submit"),
        Some(json!({}))).await;
    ali.send(&app.app, "POST", &format!("/api/workspaces/{wid}/process-instances/{b_id}/start"),
        Some(json!({}))).await;
    ali.send(&app.app, "POST", &format!("/api/workspaces/{wid}/process-instances/{b_id}/submit"),
        Some(json!({}))).await;
    let (st, _) = ali.send(&app.app, "POST", &format!("/api/workspaces/{wid}/process-instances/{b_id}/reject"),
        Some(json!({ "note": "olcu hatali" }))).await;
    assert_eq!(st, StatusCode::OK);

    // Rework: B'den yeniden uret -> kalem aktif kalmali (senkron no-op degil, active garantisi)
    let (st, _) = ali.send(&app.app, "POST", &format!("/api/workspaces/{wid}/work-items/{iid}/rework"),
        Some(json!({ "restart_from_process_id": b_id, "reason": "olcu duzeltilecek" }))).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(item_status(&mut ali, &app.app, &wid, &iid).await, "active",
        "rework sirasinda kalem aktif olmali");
}
