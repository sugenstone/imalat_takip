//! Faz 9 testleri: bildirimler + e-posta kuyrugu.

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

/// X -> Y(onayli Supervisor) akisi + is kalemi. Veli Supervisor.
async fn bootstrap() -> (TestApp, Client, Client, String, String, String, String, String) {
    let app = setup().await;
    let mut ali = Client::new();
    ali.send(&app.app, "POST", "/api/auth/register",
        Some(json!({ "email": "ali@t.com", "password": "parola123", "name": "Ali" }))).await;
    let mut veli = Client::new();
    veli.send(&app.app, "POST", "/api/auth/register",
        Some(json!({ "email": "veli@t.com", "password": "parola123", "name": "Veli" }))).await;
    let (_, me) = veli.send(&app.app, "GET", "/api/auth/me", None).await;
    let veli_uid = me["id"].as_str().unwrap().to_string();

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
        Some(json!({ "section_id": blok["id"], "name": "Is Kalemi" }))).await;
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

    let (_, inst) = ali.send(&app.app, "GET",
        &format!("/api/workspaces/{wid}/work-items/{iid}/workflow"), None).await;
    let x_id = inst["processes"].as_array().unwrap()[0]["id"].as_str().unwrap().to_string();
    let y_id = inst["processes"].as_array().unwrap()[1]["id"].as_str().unwrap().to_string();

    (app, ali, veli, wid, iid, veli_uid, x_id, y_id)
}

#[tokio::test]
async fn notifications_flow_assignment_to_read() {
    let (app, mut ali, mut veli, wid, iid, veli_uid, x_id, _y_id) = bootstrap().await;

    // X'e veli'yi ata -> bildirim
    let (st, _) = ali.send(&app.app, "POST",
        &format!("/api/workspaces/{wid}/process-instances/{x_id}/assignments"),
        Some(json!({ "assignee_type": "user", "assignee_id": veli_uid }))).await;
    assert_eq!(st, StatusCode::CREATED);

    // Veli'nin okunmamis sayisi 1 olmali
    let (st, cnt) = veli.send(&app.app, "GET",
        &format!("/api/workspaces/{wid}/notifications/unread"), None).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(cnt["unread"], 1, "atama bildirimi dusmeli: {cnt:?}");

    // Liste
    let (_, list) = veli.send(&app.app, "GET",
        &format!("/api/workspaces/{wid}/notifications"), None).await;
    let n = &list.as_array().unwrap()[0];
    assert_eq!(n["type"], "process.assigned");
    assert!(n["title"].as_str().unwrap().contains("X"));
    assert!(n["message"].as_str().unwrap().contains("Is Kalemi"));
    let nid = n["id"].as_str().unwrap().to_string();

    // Ali'nin listesi bos (kendine atama yapmadi)
    let (_, ali_list) = ali.send(&app.app, "GET",
        &format!("/api/workspaces/{wid}/notifications"), None).await;
    assert_eq!(ali_list.as_array().unwrap().len(), 0);

    // Okundu isaretle
    let (st, _) = veli.send(&app.app, "POST",
        &format!("/api/workspaces/{wid}/notifications/{nid}/read"), None).await;
    assert_eq!(st, StatusCode::OK);
    let (_, cnt2) = veli.send(&app.app, "GET",
        &format!("/api/workspaces/{wid}/notifications/unread"), None).await;
    assert_eq!(cnt2["unread"], 0);

    // Outbox: atama e-postasi kuyrukta (worker calismadigindan pending)
    let (_, outbox) = ali.send(&app.app, "GET",
        &format!("/api/workspaces/{wid}/notifications/emails"), None).await;
    let ob = outbox.as_array().unwrap();
    assert!(!ob.is_empty(), "outbox'ta e-posta olmali");
    assert_eq!(ob[0]["to_email"], "veli@t.com");
    assert!(ob[0]["subject"].as_str().unwrap().contains("X"));
    let _ = iid;
}

#[tokio::test]
async fn notifications_approval_and_result_events() {
    let (app, mut ali, mut veli, wid, _iid, _veli_uid, x_id, y_id) = bootstrap().await;

    // X tamamla, Y baslat + submit -> Supervisor'a approval_required
    for pid in [&x_id] {
        ali.send(&app.app, "POST", &format!("/api/workspaces/{wid}/process-instances/{pid}/start"),
            Some(json!({}))).await;
        ali.send(&app.app, "POST", &format!("/api/workspaces/{wid}/process-instances/{pid}/submit"),
            Some(json!({}))).await;
    }
    // Y baslangicta waiting; X approved sonrasi ready olur
    ali.send(&app.app, "POST", &format!("/api/workspaces/{wid}/process-instances/{y_id}/start"),
        Some(json!({}))).await;
    ali.send(&app.app, "POST", &format!("/api/workspaces/{wid}/process-instances/{y_id}/submit"),
        Some(json!({}))).await;

    let (_, cnt) = veli.send(&app.app, "GET",
        &format!("/api/workspaces/{wid}/notifications/unread"), None).await;
    assert_eq!(cnt["unread"], 1, "onay bildirimi Supervisor'a dusmeli: {cnt:?}");
    let (_, list) = veli.send(&app.app, "GET",
        &format!("/api/workspaces/{wid}/notifications"), None).await;
    assert_eq!(list[0]["type"], "process.approval_required");

    // Y'ye veli'yi ata (reddedilince haber alsin)
    let (_, me) = veli.send(&app.app, "GET", "/api/auth/me", None).await;
    let veli_uid = me["id"].as_str().unwrap().to_string();
    ali.send(&app.app, "POST",
        &format!("/api/workspaces/{wid}/process-instances/{y_id}/assignments"),
        Some(json!({ "assignee_type": "user", "assignee_id": veli_uid }))).await;
    // onceki okunmamislari temizle
    veli.send(&app.app, "POST",
        &format!("/api/workspaces/{wid}/notifications/read-all"), None).await;

    // REDDET -> atananlara rejected
    veli.send(&app.app, "POST", &format!("/api/workspaces/{wid}/process-instances/{y_id}/reject"),
        Some(json!({ "note": "hata" }))).await;
    let (_, list2) = veli.send(&app.app, "GET",
        &format!("/api/workspaces/{wid}/notifications"), None).await;
    assert_eq!(list2[0]["type"], "process.rejected", "red bildirimi dusmeli: {list2:?}");
    assert!(list2[0]["message"].as_str().unwrap().contains("hata"));

    // Rework: X'ten -> atananlara (X'in atamasi yok ama Y var; X atamasi yoksa bildirim gitmez)
    let (st, _) = ali.send(&app.app, "POST",
        &format!("/api/workspaces/{wid}/work-items/{_iid}/rework"),
        Some(json!({ "restart_from_process_id": x_id, "reason": "tekrar" }))).await;
    assert_eq!(st, StatusCode::OK);

    // X'e de atama yap + tekrar rework -> bildirim gelmeli
    ali.send(&app.app, "POST",
        &format!("/api/workspaces/{wid}/process-instances/{x_id}/assignments"),
        Some(json!({ "assignee_type": "user", "assignee_id": veli_uid }))).await;
    veli.send(&app.app, "POST",
        &format!("/api/workspaces/{wid}/notifications/read-all"), None).await;
    // Y'yi tekrar failed duruma getirmek icin: X tamamla, Y basla+submit, reddet
    for pid in [&x_id, &y_id] {
        ali.send(&app.app, "POST", &format!("/api/workspaces/{wid}/process-instances/{pid}/start"),
            Some(json!({}))).await;
        ali.send(&app.app, "POST", &format!("/api/workspaces/{wid}/process-instances/{pid}/submit"),
            Some(json!({}))).await;
    }
    veli.send(&app.app, "POST", &format!("/api/workspaces/{wid}/process-instances/{y_id}/reject"),
        Some(json!({ "note": "tekrar hata" }))).await;
    veli.send(&app.app, "POST",
        &format!("/api/workspaces/{wid}/notifications/read-all"), None).await;

    let (st, _) = ali.send(&app.app, "POST",
        &format!("/api/workspaces/{wid}/work-items/{_iid}/rework"),
        Some(json!({ "restart_from_process_id": x_id, "reason": "ikinci rework" }))).await;
    assert_eq!(st, StatusCode::OK);

    let (_, list3) = veli.send(&app.app, "GET",
        &format!("/api/workspaces/{wid}/notifications"), None).await;
    assert_eq!(list3[0]["type"], "rework.started", "rework bildirimi atananlara dusmeli");

    // Outbox yetkisi: Worker goremez
    let mut ayse = Client::new();
    ayse.send(&app.app, "POST", "/api/auth/register",
        Some(json!({ "email": "ayse@t.com", "password": "parola123", "name": "Ayse" }))).await;
    let (_, roles) = ali.send(&app.app, "GET", &format!("/api/workspaces/{wid}/roles"), None).await;
    let worker_id = roles.as_array().unwrap().iter()
        .find(|r| r["name"] == "Worker").map(|r| r["id"].as_str().unwrap().to_string()).unwrap();
    ali.send(&app.app, "POST", &format!("/api/workspaces/{wid}/members"),
        Some(json!({ "email": "ayse@t.com", "role_id": worker_id }))).await;
    let (st, _) = ayse.send(&app.app, "GET",
        &format!("/api/workspaces/{wid}/notifications/emails"), None).await;
    assert_eq!(st, StatusCode::FORBIDDEN);
}
