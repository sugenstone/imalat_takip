//! Faz 5 testleri: workflow runtime - atama, surec motoru, dependency cascade.

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

/// Hazir ortam: workspace + bolum + is kalemi + yayinlanmis akis (DAG'li).
/// Akis: A -> B, A -> C (paralel), B+C -> D
async fn bootstrap() -> (TestApp, Client, String, String, String, [Value; 4]) {
    let app = setup().await;
    let mut ali = Client::new();
    ali.send(&app.app, "POST", "/api/auth/register",
        Some(json!({ "email": "ali@t.com", "password": "parola123", "name": "Ali" }))).await;
    let (_, ws) = ali.send(&app.app, "POST", "/api/workspaces",
        Some(json!({ "name": "Atolye" }))).await;
    let wid = ws["id"].as_str().unwrap().to_string();

    let (_, blok) = ali.send(&app.app, "POST", &format!("/api/workspaces/{wid}/sections"),
        Some(json!({ "name": "Blok A" }))).await;
    let blok_id = blok["id"].as_str().unwrap().to_string();

    let (_, item) = ali.send(&app.app, "POST", &format!("/api/workspaces/{wid}/work-items"),
        Some(json!({ "section_id": blok_id, "name": "Ana Is" }))).await;
    let iid = item["id"].as_str().unwrap().to_string();

    // Akis kur: A, B, C, D
    let (_, wf) = ali.send(&app.app, "POST", &format!("/api/workspaces/{wid}/workflows"),
        Some(json!({ "name": "Test Akis" }))).await;
    let tfid = wf["id"].as_str().unwrap().to_string();

    let mut nodes = Vec::new();
    for name in ["A", "B", "C", "D"] {
        let (_, n) = ali.send(&app.app, "POST",
            &format!("/api/workspaces/{wid}/workflows/{tfid}/draft/nodes"),
            Some(json!({ "name": name }))).await;
        nodes.push(n);
    }
    let ids: Vec<String> = nodes.iter().map(|n| n["id"].as_str().unwrap().to_string()).collect();
    for (p, s) in [(0, 1), (0, 2), (1, 3), (2, 3)] {
        let (st, _) = ali.send(&app.app, "POST",
            &format!("/api/workspaces/{wid}/workflows/{tfid}/draft/dependencies"),
            Some(json!({ "predecessor_node_id": ids[p], "successor_node_id": ids[s] }))).await;
        assert_eq!(st, StatusCode::CREATED);
    }
    let (st, _) = ali.send(&app.app, "POST",
        &format!("/api/workspaces/{wid}/workflows/{tfid}/publish"), None).await;
    assert_eq!(st, StatusCode::OK);

    let nodes: [Value; 4] = [
        nodes[0].clone(),
        nodes[1].clone(),
        nodes[2].clone(),
        nodes[3].clone(),
    ];
    (app, ali, wid, iid, tfid, nodes)
}

fn proc_by_name(v: &Value, name: &str) -> Value {
    v["processes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["name"] == name)
        .cloned()
        .unwrap()
}

#[tokio::test]
async fn runtime_assignment_and_cascade() {
    let (app, mut ali, wid, iid, tfid, _nodes) = bootstrap().await;

    // --- Atama ---
    let (st, inst) = ali.send(&app.app, "POST",
        &format!("/api/workspaces/{wid}/work-items/{iid}/workflow"),
        Some(json!({ "template_id": tfid }))).await;
    assert_eq!(st, StatusCode::CREATED, "atama basarili: {inst:?}");
    assert_eq!(inst["status"], "active");
    assert_eq!(inst["version_number"], 1);
    assert_eq!(inst["total"], 4);
    assert_eq!(inst["approved"], 0);

    // Kok adim A ready, digerleri waiting
    assert_eq!(proc_by_name(&inst, "A")["status"], "ready");
    assert_eq!(proc_by_name(&inst, "B")["status"], "waiting");
    assert_eq!(proc_by_name(&inst, "C")["status"], "waiting");
    assert_eq!(proc_by_name(&inst, "D")["status"], "waiting");

    // Cift atama engellenir
    let (st, _) = ali.send(&app.app, "POST",
        &format!("/api/workspaces/{wid}/work-items/{iid}/workflow"),
        Some(json!({ "template_id": tfid }))).await;
    assert_eq!(st, StatusCode::CONFLICT);

    // Yayinlanmamis template ile atama engellenir
    let (_, wf2) = ali.send(&app.app, "POST", &format!("/api/workspaces/{wid}/workflows"),
        Some(json!({ "name": "Bos Akis" }))).await;
    let (st, _) = ali.send(&app.app, "POST",
        &format!("/api/workspaces/{wid}/work-items/{iid}/workflow"),
        Some(json!({ "template_id": wf2["id"] }))).await;
    assert_eq!(st, StatusCode::BAD_REQUEST);

    // --- A: ready degilken start edilemez ---
    let a_id = proc_by_name(&inst, "A")["id"].as_str().unwrap().to_string();
    let b_id = proc_by_name(&inst, "B")["id"].as_str().unwrap().to_string();
    let c_id = proc_by_name(&inst, "C")["id"].as_str().unwrap().to_string();
    let d_id = proc_by_name(&inst, "D")["id"].as_str().unwrap().to_string();

    let (st, _) = ali.send(&app.app, "POST",
        &format!("/api/workspaces/{wid}/process-instances/{b_id}/start"),
        Some(json!({}))).await;
    assert_eq!(st, StatusCode::BAD_REQUEST, "waiting surec start edilemez");

    // --- A'yi baslat + tamamla ---
    let (st, _) = ali.send(&app.app, "POST",
        &format!("/api/workspaces/{wid}/process-instances/{a_id}/start"),
        Some(json!({}))).await;
    assert_eq!(st, StatusCode::OK);

    // in_progress iken tekrar start edilemez
    let (st, _) = ali.send(&app.app, "POST",
        &format!("/api/workspaces/{wid}/process-instances/{a_id}/start"),
        Some(json!({}))).await;
    assert_eq!(st, StatusCode::BAD_REQUEST);

    // submit oncesinde ready olmayan kontrol zaten var; A'yi submit et
    let (st, _) = ali.send(&app.app, "POST",
        &format!("/api/workspaces/{wid}/process-instances/{a_id}/submit"),
        Some(json!({}))).await;
    assert_eq!(st, StatusCode::OK);

    // CASCADE: B ve C ready olmali, D hala waiting (B ve C ikisi de bitmeli)
    let (_, inst2) = ali.send(&app.app, "GET",
        &format!("/api/workspaces/{wid}/work-items/{iid}/workflow"), None).await;
    assert_eq!(inst2["approved"], 1);
    assert_eq!(proc_by_name(&inst2, "B")["status"], "ready", "B ready olmali");
    assert_eq!(proc_by_name(&inst2, "C")["status"], "ready", "C ready olmali");
    assert_eq!(proc_by_name(&inst2, "D")["status"], "waiting", "D hala beklemeli");

    // --- Paralel dal: B tamamlansin, D hala waiting ---
    for pid in [&b_id] {
        ali.send(&app.app, "POST",
            &format!("/api/workspaces/{wid}/process-instances/{pid}/start"),
            Some(json!({}))).await;
        ali.send(&app.app, "POST",
            &format!("/api/workspaces/{wid}/process-instances/{pid}/submit"),
            Some(json!({}))).await;
    }
    let (_, inst3) = ali.send(&app.app, "GET",
        &format!("/api/workspaces/{wid}/work-items/{iid}/workflow"), None).await;
    assert_eq!(proc_by_name(&inst3, "D")["status"], "waiting", "C bitmeden D beklemeli");

    // --- C tamamlaninca D ready ---
    ali.send(&app.app, "POST",
        &format!("/api/workspaces/{wid}/process-instances/{c_id}/start"),
        Some(json!({}))).await;
    ali.send(&app.app, "POST",
        &format!("/api/workspaces/{wid}/process-instances/{c_id}/submit"),
        Some(json!({}))).await;
    let (_, inst4) = ali.send(&app.app, "GET",
        &format!("/api/workspaces/{wid}/work-items/{iid}/workflow"), None).await;
    assert_eq!(proc_by_name(&inst4, "D")["status"], "ready", "her iki dal bitince D ready");
    assert_eq!(inst4["status"], "active");

    // --- D tamamlaninca WORKFLOW COMPLETED ---
    ali.send(&app.app, "POST",
        &format!("/api/workspaces/{wid}/process-instances/{d_id}/start"),
        Some(json!({}))).await;
    let (st, _) = ali.send(&app.app, "POST",
        &format!("/api/workspaces/{wid}/process-instances/{d_id}/submit"),
        Some(json!({}))).await;
    assert_eq!(st, StatusCode::OK);

    let (_, inst5) = ali.send(&app.app, "GET",
        &format!("/api/workspaces/{wid}/work-items/{iid}/workflow"), None).await;
    assert_eq!(inst5["status"], "completed", "tum surecler bitince instance completed");
    assert_eq!(inst5["approved"], 4);
    assert!(inst5["completed_at"].is_string());

    // Tamamlanan instance'da surec islemi yapilamaz
    let (st, _) = ali.send(&app.app, "POST",
        &format!("/api/workspaces/{wid}/process-instances/{d_id}/start"),
        Some(json!({}))).await;
    assert_eq!(st, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn runtime_cancel_and_permissions() {
    let (app, mut ali, wid, iid, tfid, _nodes) = bootstrap().await;

    // Veli Viewer olarak eklensin
    let mut veli = Client::new();
    veli.send(&app.app, "POST", "/api/auth/register",
        Some(json!({ "email": "veli@t.com", "password": "parola123", "name": "Veli" }))).await;
    let (_, roles) = ali.send(&app.app, "GET", &format!("/api/workspaces/{wid}/roles"), None).await;
    let viewer_id = roles.as_array().unwrap().iter()
        .find(|r| r["name"] == "Viewer").map(|r| r["id"].as_str().unwrap().to_string()).unwrap();
    ali.send(&app.app, "POST", &format!("/api/workspaces/{wid}/members"),
        Some(json!({ "email": "veli@t.com", "role_id": viewer_id }))).await;

    // Viewer atama yapamaz
    let (st, _) = veli.send(&app.app, "POST",
        &format!("/api/workspaces/{wid}/work-items/{iid}/workflow"),
        Some(json!({ "template_id": tfid }))).await;
    assert_eq!(st, StatusCode::FORBIDDEN);

    // ali atar
    let (_, inst) = ali.send(&app.app, "POST",
        &format!("/api/workspaces/{wid}/work-items/{iid}/workflow"),
        Some(json!({ "template_id": tfid }))).await;
    let a_id = proc_by_name(&inst, "A")["id"].as_str().unwrap().to_string();

    // Viewer surec baslatamaz (process.start yok)
    let (st, _) = veli.send(&app.app, "POST",
        &format!("/api/workspaces/{wid}/process-instances/{a_id}/start"),
        Some(json!({}))).await;
    assert_eq!(st, StatusCode::FORBIDDEN);

    // Supervisor ekibine yukselt -> baslatabilir ama complete izni Supervisor'da VAR
    let (_, members) = ali.send(&app.app, "GET", &format!("/api/workspaces/{wid}/members"), None).await;
    let veli_mid = members.as_array().unwrap().iter()
        .find(|m| m["email"] == "veli@t.com").map(|m| m["id"].as_str().unwrap().to_string()).unwrap();
    let sup_id = roles.as_array().unwrap().iter()
        .find(|r| r["name"] == "Supervisor").map(|r| r["id"].as_str().unwrap().to_string()).unwrap();
    ali.send(&app.app, "PATCH", &format!("/api/workspaces/{wid}/members/{veli_mid}"),
        Some(json!({ "role_id": sup_id }))).await;

    let (st, _) = veli.send(&app.app, "POST",
        &format!("/api/workspaces/{wid}/process-instances/{a_id}/start"),
        Some(json!({}))).await;
    assert_eq!(st, StatusCode::OK, "Supervisor surec baslatabilmeli");

    // Scope disindan erisim: veli'yi baska subtree ile sinirla
    let (_, blok_b) = ali.send(&app.app, "POST", &format!("/api/workspaces/{wid}/sections"),
        Some(json!({ "name": "Blok B" }))).await;
    ali.send(&app.app, "PUT", &format!("/api/workspaces/{wid}/members/{veli_mid}/scopes"),
        Some(json!({ "section_ids": [blok_b["id"]] }))).await;

    let (st, _) = veli.send(&app.app, "POST",
        &format!("/api/workspaces/{wid}/process-instances/{a_id}/submit"),
        Some(json!({}))).await;
    assert_eq!(st, StatusCode::FORBIDDEN, "kapsam disi submit engellenmeli");

    // ali devam ettirir ve IPTAL eder
    ali.send(&app.app, "POST",
        &format!("/api/workspaces/{wid}/process-instances/{a_id}/submit"),
        Some(json!({}))).await;
    let (st, _) = ali.send(&app.app, "DELETE",
        &format!("/api/workspaces/{wid}/work-items/{iid}/workflow"), None).await;
    assert_eq!(st, StatusCode::OK);

    let (_, inst2) = ali.send(&app.app, "GET",
        &format!("/api/workspaces/{wid}/work-items/{iid}/workflow"), None).await;
    assert_eq!(inst2["status"], "cancelled");
    // A approved kaldi (gecmis korunur), B/C/D cancelled
    assert_eq!(proc_by_name(&inst2, "A")["status"], "approved");
    assert_eq!(proc_by_name(&inst2, "B")["status"], "cancelled");
    assert_eq!(proc_by_name(&inst2, "D")["status"], "cancelled");

    // Iptal sonrasi yeni atama yapilabilir
    let (st, _) = ali.send(&app.app, "POST",
        &format!("/api/workspaces/{wid}/work-items/{iid}/workflow"),
        Some(json!({ "template_id": tfid }))).await;
    assert_eq!(st, StatusCode::CREATED, "iptal sonrasi yeniden atanabilmeli");
}
