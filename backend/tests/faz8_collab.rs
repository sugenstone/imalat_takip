//! Faz 8 testleri: yorumlar, dosya ekleri, audit log.

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

    /// Multipart dosya yukleme.
    async fn upload(
        &mut self,
        app: &Router,
        uri: &str,
        entity_type: &str,
        entity_id: &str,
        file_name: &str,
        content: &[u8],
    ) -> (StatusCode, Value) {
        let boundary = "----imtktest1234";
        let mut body = Vec::new();
        for (name, val) in [("entity_type", entity_type.as_bytes()), ("entity_id", entity_id.as_bytes())] {
            body.extend_from_slice(format!("--{boundary}\r\ncontent-disposition: form-data; name=\"{name}\"\r\n\r\n").as_bytes());
            body.extend_from_slice(val);
            body.extend_from_slice(b"\r\n");
        }
        body.extend_from_slice(
            format!("--{boundary}\r\ncontent-disposition: form-data; name=\"file\"; filename=\"{file_name}\"\r\ncontent-type: image/png\r\n\r\n").as_bytes(),
        );
        body.extend_from_slice(content);
        body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());

        let mut builder = Request::builder().method("POST").uri(uri).header(
            header::CONTENT_TYPE,
            format!("multipart/form-data; boundary={boundary}"),
        );
        if let Some(c) = &self.cookie {
            builder = builder.header(header::COOKIE, c);
        }
        let req = builder.body(Body::from(body)).unwrap();
        let res = app.clone().oneshot(req).await.unwrap();
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

async fn bootstrap() -> (TestApp, Client, String, String) {
    let app = setup().await;
    let mut ali = Client::new();
    ali.send(&app.app, "POST", "/api/auth/register",
        Some(json!({ "email": "ali@t.com", "password": "parola123", "name": "Ali" }))).await;
    let (_, ws) = ali.send(&app.app, "POST", "/api/workspaces",
        Some(json!({ "name": "Atolye" }))).await;
    let wid = ws["id"].as_str().unwrap().to_string();
    let (_, blok) = ali.send(&app.app, "POST", &format!("/api/workspaces/{wid}/sections"),
        Some(json!({ "name": "Blok A" }))).await;
    let (_, item) = ali.send(&app.app, "POST", &format!("/api/workspaces/{wid}/work-items"),
        Some(json!({ "section_id": blok["id"], "name": "Ana Is" }))).await;
    let iid = item["id"].as_str().unwrap().to_string();
    (app, ali, wid, iid)
}

#[tokio::test]
async fn comments_crud_and_scope() {
    let (app, mut ali, wid, iid) = bootstrap().await;

    // Is kalemine yorum ekle
    let (st, c1) = ali.send(&app.app, "POST", &format!("/api/workspaces/{wid}/comments"),
        Some(json!({ "entity_type": "work_item", "entity_id": iid, "body": "Ilk yorum" }))).await;
    assert_eq!(st, StatusCode::CREATED);
    assert_eq!(c1["author_name"], "Ali");
    assert_eq!(c1["body"], "Ilk yorum");

    // Bos yorum reddedilir
    let (st, _) = ali.send(&app.app, "POST", &format!("/api/workspaces/{wid}/comments"),
        Some(json!({ "entity_type": "work_item", "entity_id": iid, "body": "  " }))).await;
    assert_eq!(st, StatusCode::BAD_REQUEST);

    // Gecersiz entity
    let (st, _) = ali.send(&app.app, "POST", &format!("/api/workspaces/{wid}/comments"),
        Some(json!({ "entity_type": "bilmemne", "entity_id": iid, "body": "x" }))).await;
    assert_eq!(st, StatusCode::BAD_REQUEST);

    // Liste
    let (st, list) = ali.send(&app.app, "GET",
        &format!("/api/workspaces/{wid}/comments?entity_type=work_item&entity_id={iid}"), None).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(list.as_array().unwrap().len(), 1);

    // Duzenle
    let cid = c1["id"].as_str().unwrap().to_string();
    let (st, edited) = ali.send(&app.app, "PATCH",
        &format!("/api/workspaces/{wid}/comments/{cid}"),
        Some(json!({ "body": "Duzenlendi" }))).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(edited["body"], "Duzenlendi");
    assert!(edited["edited_at"].is_string());

    // Baskasinin yorumunu duzenleyemez
    let mut veli = Client::new();
    veli.send(&app.app, "POST", "/api/auth/register",
        Some(json!({ "email": "veli@t.com", "password": "parola123", "name": "Veli" }))).await;
    let (_, roles) = ali.send(&app.app, "GET", &format!("/api/workspaces/{wid}/roles"), None).await;
    let worker_id = roles.as_array().unwrap().iter()
        .find(|r| r["name"] == "Worker").map(|r| r["id"].as_str().unwrap().to_string()).unwrap();
    ali.send(&app.app, "POST", &format!("/api/workspaces/{wid}/members"),
        Some(json!({ "email": "veli@t.com", "role_id": worker_id }))).await;
    let (st, _) = veli.send(&app.app, "PATCH",
        &format!("/api/workspaces/{wid}/comments/{cid}"),
        Some(json!({ "body": "hack" }))).await;
    assert_eq!(st, StatusCode::FORBIDDEN);

    // Scope disindan yorum yapilamaz
    let (_, blok_b) = ali.send(&app.app, "POST", &format!("/api/workspaces/{wid}/sections"),
        Some(json!({ "name": "Blok B" }))).await;
    let (_, members) = ali.send(&app.app, "GET", &format!("/api/workspaces/{wid}/members"), None).await;
    let veli_mid = members.as_array().unwrap().iter()
        .find(|m| m["email"] == "veli@t.com").map(|m| m["id"].as_str().unwrap().to_string()).unwrap();
    ali.send(&app.app, "PUT", &format!("/api/workspaces/{wid}/members/{veli_mid}/scopes"),
        Some(json!({ "section_ids": [blok_b["id"]] }))).await;
    let (st, _) = veli.send(&app.app, "POST", &format!("/api/workspaces/{wid}/comments"),
        Some(json!({ "entity_type": "work_item", "entity_id": iid, "body": "izinsiz" }))).await;
    assert_eq!(st, StatusCode::FORBIDDEN, "kapsam disi yorum engellenmeli");
}

#[tokio::test]
async fn attachments_upload_download_delete() {
    let (app, mut ali, wid, iid) = bootstrap().await;
    std::env::set_var("IMTK_UPLOAD_DIR", std::env::temp_dir().join("imtk_test_uploads").to_str().unwrap());

    let png: Vec<u8> = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 1, 2, 3, 4];

    // Yukle
    let (st, att) = ali.upload(&app.app, &format!("/api/workspaces/{wid}/attachments"),
        "work_item", &iid, "foto.png", &png).await;
    assert_eq!(st, StatusCode::CREATED, "yukleme basarili: {att:?}");
    assert_eq!(att["file_name"], "foto.png");
    assert_eq!(att["size"], png.len() as i64);
    let aid = att["id"].as_str().unwrap().to_string();

    // Liste
    let (st, list) = ali.send(&app.app, "GET",
        &format!("/api/workspaces/{wid}/attachments?entity_type=work_item&entity_id={iid}"), None).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(list.as_array().unwrap().len(), 1);

    // Indir (icerik dogru mu)
    let mut builder = Request::builder().method("GET")
        .uri(&format!("/api/workspaces/{wid}/attachments/{aid}/file"));
    if let Some(c) = &ali.cookie {
        builder = builder.header(header::COOKIE, c);
    }
    let res = app.app.clone().oneshot(builder.body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(res.into_body(), 1024 * 1024).await.unwrap();
    assert_eq!(bytes.to_vec(), png);

    // exe reddedilir
    let (st, _) = ali.upload(&app.app, &format!("/api/workspaces/{wid}/attachments"),
        "work_item", &iid, "virus.exe", b"MZ....").await;
    assert_eq!(st, StatusCode::BAD_REQUEST);

    // Gecersiz entity reddedilir
    let (st, _) = ali.upload(&app.app, &format!("/api/workspaces/{wid}/attachments"),
        "work_item", "yok-boyle", "a.png", &png).await;
    assert_eq!(st, StatusCode::NOT_FOUND);

    // Arsivle (soft)
    let (st, _) = ali.send(&app.app, "DELETE",
        &format!("/api/workspaces/{wid}/attachments/{aid}"), None).await;
    assert_eq!(st, StatusCode::OK);
    let (_, list2) = ali.send(&app.app, "GET",
        &format!("/api/workspaces/{wid}/attachments?entity_type=work_item&entity_id={iid}"), None).await;
    assert_eq!(list2.as_array().unwrap().len(), 0, "arsivlenen listeden dusmeli");
}

#[tokio::test]
async fn audit_records_critical_actions() {
    let (app, mut ali, mut veli, wid, _iid, _tfid, y_id) = setup_approval_flow().await;

    // veli onaylar -> audit'e dusmeli
    let (st, _) = veli.send(&app.app, "POST",
        &format!("/api/workspaces/{wid}/process-instances/{y_id}/approve"),
        Some(json!({}))).await;
    let _ = st;

    // Owner audit goruntuleyebilir
    let (st, logs) = ali.send(&app.app, "GET",
        &format!("/api/workspaces/{wid}/audit?limit=50"), None).await;
    assert_eq!(st, StatusCode::OK, "Owner audit gorebilmeli");
    let logs = logs.as_array().unwrap();
    assert!(!logs.is_empty(), "audit kayitlari olusmali");

    let actions: Vec<&str> = logs.iter().map(|l| l["action"].as_str().unwrap()).collect();
    assert!(actions.contains(&"member.added"), "uye ekleme audit'te olmali: {actions:?}");
    assert!(actions.contains(&"workflow.published"), "publish audit'te olmali");
    assert!(actions.contains(&"workflow.assigned"), "atama audit'te olmali");
    assert!(actions.contains(&"process.started"), "surec baslatma audit'te olmali");
    assert!(actions.contains(&"process.rejected"), "red audit'te olmali");
    assert!(actions.contains(&"process.approved"), "onay audit'te olmali");

    // aktor isimleri dolu
    let approved_log = logs.iter().find(|l| l["action"] == "process.approved").unwrap();
    assert_eq!(approved_log["actor_name"], "Veli");

    // Worker audit goremez
    let mut ayse = Client::new();
    ayse.send(&app.app, "POST", "/api/auth/register",
        Some(json!({ "email": "ayse@t.com", "password": "parola123", "name": "Ayse" }))).await;
    let (_, roles) = ali.send(&app.app, "GET", &format!("/api/workspaces/{wid}/roles"), None).await;
    let worker_id = roles.as_array().unwrap().iter()
        .find(|r| r["name"] == "Worker").map(|r| r["id"].as_str().unwrap().to_string()).unwrap();
    ali.send(&app.app, "POST", &format!("/api/workspaces/{wid}/members"),
        Some(json!({ "email": "ayse@t.com", "role_id": worker_id }))).await;
    let (st, _) = ayse.send(&app.app, "GET", &format!("/api/workspaces/{wid}/audit"), None).await;
    assert_eq!(st, StatusCode::FORBIDDEN, "Worker audit goremez");
}

/// X -> Y(onayli Supervisor) akisi; Y reddedilmis durumda doner.
/// (approve islemini test cagirir)
async fn setup_approval_flow() -> (TestApp, Client, Client, String, String, String, String) {
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
    let (_, item) = ali.send(&app.app, "POST", &format!("/api/workspaces/{wid}/work-items"),
        Some(json!({ "section_id": blok["id"], "name": "Is" }))).await;
    let iid = item["id"].as_str().unwrap().to_string();

    let (_, wf) = ali.send(&app.app, "POST", &format!("/api/workspaces/{wid}/workflows"),
        Some(json!({ "name": "Akis" }))).await;
    let tfid = wf["id"].as_str().unwrap().to_string();
    let (_, nx) = ali.send(&app.app, "POST",
        &format!("/api/workspaces/{wid}/workflows/{tfid}/draft/nodes"),
        Some(json!({ "name": "X" }))).await;
    let (_, ny) = ali.send(&app.app, "POST",
        &format!("/api/workspaces/{wid}/workflows/{tfid}/draft/nodes"),
        Some(json!({ "name": "Y" }))).await;
    ali.send(&app.app, "PATCH",
        &format!("/api/workspaces/{wid}/workflows/{tfid}/draft/nodes/{}", ny["id"].as_str().unwrap()),
        Some(json!({ "approval_rule": { "required": true, "approver_role_id": sup_id } }))).await;
    ali.send(&app.app, "POST",
        &format!("/api/workspaces/{wid}/workflows/{tfid}/draft/dependencies"),
        Some(json!({ "predecessor_node_id": nx["id"], "successor_node_id": ny["id"] }))).await;
    ali.send(&app.app, "POST", &format!("/api/workspaces/{wid}/workflows/{tfid}/publish"), None).await;

    ali.send(&app.app, "POST", &format!("/api/workspaces/{wid}/work-items/{iid}/workflow"),
        Some(json!({ "template_id": tfid }))).await;

    // X basla+tamamla, Y basla+submit -> reddet
    let (_, i1) = ali.send(&app.app, "GET",
        &format!("/api/workspaces/{wid}/work-items/{iid}/workflow"), None).await;
    let x_id = i1["processes"].as_array().unwrap()[0]["id"].as_str().unwrap().to_string();
    let y_id = i1["processes"].as_array().unwrap()[1]["id"].as_str().unwrap().to_string();
    for pid in [&x_id, &y_id] {
        ali.send(&app.app, "POST", &format!("/api/workspaces/{wid}/process-instances/{pid}/start"),
            Some(json!({}))).await;
        ali.send(&app.app, "POST", &format!("/api/workspaces/{wid}/process-instances/{pid}/submit"),
            Some(json!({}))).await;
    }
    veli.send(&app.app, "POST", &format!("/api/workspaces/{wid}/process-instances/{y_id}/reject"),
        Some(json!({ "note": "hatali" }))).await;

    // Approve edilecek sureci global state'te tutmak yerine test icinde tekrar reddi kaldiramayiz;
    // approve testi icin: rework ile yeniden akis -> submit -> approve
    let (st, _) = ali.send(&app.app, "POST",
        &format!("/api/workspaces/{wid}/work-items/{iid}/rework"),
        Some(json!({ "restart_from_process_id": x_id, "reason": "tekrar" }))).await;
    assert_eq!(st, StatusCode::OK);

    for pid in [&x_id, &y_id] {
        ali.send(&app.app, "POST", &format!("/api/workspaces/{wid}/process-instances/{pid}/start"),
            Some(json!({}))).await;
        ali.send(&app.app, "POST", &format!("/api/workspaces/{wid}/process-instances/{pid}/submit"),
            Some(json!({}))).await;
    }

    (app, ali, veli, wid, iid, tfid, y_id)
}
