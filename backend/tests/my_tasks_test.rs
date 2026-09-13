//! my-tasks testleri: gorevlerim (atama/takim) + onay yetkisi filtresi.

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

/// Ali owner; Veli Supervisor; Ayse Worker.
/// Akis: A -> B(onayli Supervisor) ; Veli A'ya atanir.
async fn bootstrap() -> (TestApp, Client, Client, Client, String, String, String, String) {
    let app = setup().await;
    let mut ali = Client::new();
    ali.send(&app.app, "POST", "/api/auth/register",
        Some(json!({ "email": "ali@t.com", "password": "parola123", "name": "Ali" }))).await;
    let mut veli = Client::new();
    veli.send(&app.app, "POST", "/api/auth/register",
        Some(json!({ "email": "veli@t.com", "password": "parola123", "name": "Veli" }))).await;
    let (_, me_v) = veli.send(&app.app, "GET", "/api/auth/me", None).await;
    let veli_uid = me_v["id"].as_str().unwrap().to_string();
    let mut ayse = Client::new();
    ayse.send(&app.app, "POST", "/api/auth/register",
        Some(json!({ "email": "ayse@t.com", "password": "parola123", "name": "Ayse" }))).await;
    let (_, me_a) = ayse.send(&app.app, "GET", "/api/auth/me", None).await;
    let ayse_uid = me_a["id"].as_str().unwrap().to_string();

    let (_, ws) = ali.send(&app.app, "POST", "/api/workspaces",
        Some(json!({ "name": "Atolye" }))).await;
    let wid = ws["id"].as_str().unwrap().to_string();

    let (_, roles) = ali.send(&app.app, "GET", &format!("/api/workspaces/{wid}/roles"), None).await;
    let sup_id = roles.as_array().unwrap().iter()
        .find(|r| r["name"] == "Supervisor").map(|r| r["id"].as_str().unwrap().to_string()).unwrap();
    let worker_id = roles.as_array().unwrap().iter()
        .find(|r| r["name"] == "Worker").map(|r| r["id"].as_str().unwrap().to_string()).unwrap();
    ali.send(&app.app, "POST", &format!("/api/workspaces/{wid}/members"),
        Some(json!({ "email": "veli@t.com", "role_id": sup_id }))).await;
    ali.send(&app.app, "POST", &format!("/api/workspaces/{wid}/members"),
        Some(json!({ "email": "ayse@t.com", "role_id": worker_id }))).await;

    let (_, blok) = ali.send(&app.app, "POST", &format!("/api/workspaces/{wid}/sections"),
        Some(json!({ "name": "Blok" }))).await;
    let (_, item) = ali.send(&app.app, "POST", &format!("/api/workspaces/{wid}/work-items"),
        Some(json!({ "section_id": blok["id"], "name": "Is Kalemi" }))).await;
    let iid = item["id"].as_str().unwrap().to_string();

    // quick-flow ile hizli kur: A -> B(onayli, kural rolu Supervisor)
    ali.send(&app.app, "POST", &format!("/api/workspaces/{wid}/quick-flow"),
        Some(json!({
            "name": "Akis",
            "steps": [ { "name": "A" }, { "name": "B", "requires_approval": true, "approver_role_id": sup_id } ],
            "assign_to_item_id": iid
        }))).await;

    // A'ya Veli'yi ata (user)
    let (_, inst) = ali.send(&app.app, "GET",
        &format!("/api/workspaces/{wid}/work-items/{iid}/workflow"), None).await;
    let a_id = inst["processes"].as_array().unwrap()[0]["id"].as_str().unwrap().to_string();
    let b_id = inst["processes"].as_array().unwrap()[1]["id"].as_str().unwrap().to_string();
    ali.send(&app.app, "POST", &format!("/api/workspaces/{wid}/process-instances/{a_id}/assignments"),
        Some(json!({ "assignee_type": "user", "assignee_id": veli_uid }))).await;

    (app, ali, veli, ayse, wid, iid, a_id, b_id)
}

#[tokio::test]
async fn my_tasks_ready_start_complete_flow() {
    let (app, _ali, mut veli, _ayse, wid, _iid, a_id, b_id) = bootstrap().await;

    // Veli'nin Gorevlerim: A Hazir
    let (st, tasks) = veli.send(&app.app, "GET", &format!("/api/workspaces/{wid}/my-tasks"), None).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(tasks["ready"].as_array().unwrap().len(), 1, "A hazir olmali: {tasks:?}");
    assert_eq!(tasks["ready"][0]["step_name"], "A");
    assert_eq!(tasks["ready"][0]["item_name"], "Is Kalemi");
    assert_eq!(tasks["in_progress"].as_array().unwrap().len(), 0);
    assert_eq!(tasks["approvals"].as_array().unwrap().len(), 0);
    // atama chip bilgisi
    let assigns: Vec<Value> = serde_json::from_str(tasks["ready"][0]["assignments"].as_str().unwrap()).unwrap();
    assert_eq!(assigns[0]["name"], "Veli");

    // Listedeki id ile sureci BASLAT (tek dokunus simülasyonu — ayni endpoint)
    veli.send(&app.app, "POST", &format!("/api/workspaces/{wid}/process-instances/{a_id}/start"),
        Some(json!({}))).await;

    // Artik in_progress'te
    let (_, t2) = veli.send(&app.app, "GET", &format!("/api/workspaces/{wid}/my-tasks"), None).await;
    assert_eq!(t2["ready"].as_array().unwrap().len(), 0);
    assert_eq!(t2["in_progress"].as_array().unwrap().len(), 1);

    // TAMAMLA (A kuralsiz -> direkt approved -> B ready olur)
    veli.send(&app.app, "POST", &format!("/api/workspaces/{wid}/process-instances/{a_id}/submit"),
        Some(json!({}))).await;
    let (_, t3) = veli.send(&app.app, "GET", &format!("/api/workspaces/{wid}/my-tasks"), None).await;
    assert_eq!(t3["in_progress"].as_array().unwrap().len(), 0);
    // B artik ready — baslat + submit -> onay bekler
    veli.send(&app.app, "POST", &format!("/api/workspaces/{wid}/process-instances/{b_id}/start"),
        Some(json!({}))).await;
    veli.send(&app.app, "POST", &format!("/api/workspaces/{wid}/process-instances/{b_id}/submit"),
        Some(json!({}))).await;
    let (_, t4) = veli.send(&app.app, "GET", &format!("/api/workspaces/{wid}/my-tasks"), None).await;
    assert_eq!(t4["approvals"].as_array().unwrap().len(), 1, "B onay listesinde olmali");
    assert_eq!(t4["approvals"][0]["step_name"], "B");
}

#[tokio::test]
async fn my_tasks_team_assignment_and_permissions() {
    let (app, mut ali, mut veli, mut ayse, wid, _iid, a_id, b_id) = bootstrap().await;

    // Ayse (Worker) gorev gormez (atanmadi)
    let (_, ta) = ayse.send(&app.app, "GET", &format!("/api/workspaces/{wid}/my-tasks"), None).await;
    assert_eq!(ta["ready"].as_array().unwrap().len(), 0);
    assert_eq!(ta["in_progress"].as_array().unwrap().len(), 0);
    assert_eq!(ta["approvals"].as_array().unwrap().len(), 0, "Worker onay gormemeli");

    // Takim kur + Ayse uye + A'ya takim ata → Ayse gorev gormeli
    let (_, team) = ali.send(&app.app, "POST", &format!("/api/workspaces/{wid}/teams"),
        Some(json!({ "name": "Ekip" }))).await;
    let (_, me_a) = ayse.send(&app.app, "GET", "/api/auth/me", None).await;
    ali.send(&app.app, "POST",
        &format!("/api/workspaces/{wid}/teams/{}/members", team["id"].as_str().unwrap()),
        Some(json!({ "user_id": me_a["id"] }))).await;
    ali.send(&app.app, "POST", &format!("/api/workspaces/{wid}/process-instances/{a_id}/assignments"),
        Some(json!({ "assignee_type": "team", "assignee_id": team["id"] }))).await;

    let (_, ta2) = ayse.send(&app.app, "GET", &format!("/api/workspaces/{wid}/my-tasks"), None).await;
    assert_eq!(ta2["ready"].as_array().unwrap().len(), 1, "takim atamasi gorunmeli");

    // Owner her seyi onaylayabilir (izni + owner)
    let (_, t_al) = ali.send(&app.app, "GET", &format!("/api/workspaces/{wid}/my-tasks"), None).await;
    assert_eq!(t_al["approvals"].as_array().unwrap().len(), 0, "submit yokken onay da yok");

    // Veli A'yi basla+tamamla (kuralsiz → approved, B ready) sonra B'yi basla+submit (onaya gider)
    veli.send(&app.app, "POST", &format!("/api/workspaces/{wid}/process-instances/{a_id}/start"),
        Some(json!({}))).await;
    veli.send(&app.app, "POST", &format!("/api/workspaces/{wid}/process-instances/{a_id}/submit"),
        Some(json!({}))).await;
    veli.send(&app.app, "POST", &format!("/api/workspaces/{wid}/process-instances/{b_id}/start"),
        Some(json!({}))).await;
    veli.send(&app.app, "POST", &format!("/api/workspaces/{wid}/process-instances/{b_id}/submit"),
        Some(json!({}))).await;

    let (_, tv) = veli.send(&app.app, "GET", &format!("/api/workspaces/{wid}/my-tasks"), None).await;
    assert_eq!(tv["approvals"].as_array().unwrap().len(), 1);

    let (_, tak) = ayse.send(&app.app, "GET", &format!("/api/workspaces/{wid}/my-tasks"), None).await;
    assert_eq!(tak["approvals"].as_array().unwrap().len(), 0, "Worker izinsiz onay gormez");

    let (_, to) = ali.send(&app.app, "GET", &format!("/api/workspaces/{wid}/my-tasks"), None).await;
    assert_eq!(to["approvals"].as_array().unwrap().len(), 1, "Owner onay gorebilir");
    assert_eq!(to["approvals"][0]["step_name"], "B");
    assert!(to["approvals"][0]["requested_by_name"].is_string(), "isteyen bilgisi dolu");

    // Kural rolu uyuşmazsa: Worker'a process.approve izni ekle ama kural Supervisor — gormemeli
    let (_, roles) = ali.send(&app.app, "GET", &format!("/api/workspaces/{wid}/roles"), None).await;
    let worker_role = roles.as_array().unwrap().iter().find(|r| r["name"] == "Worker").unwrap();
    ali.send(&app.app, "PATCH", &format!("/api/workspaces/{wid}/roles/{}", worker_role["id"].as_str().unwrap()),
        Some(json!({ "permission_keys": ["process.start", "process.complete", "process.approve"] }))).await;
    let (_, tak2) = ayse.send(&app.app, "GET", &format!("/api/workspaces/{wid}/my-tasks"), None).await;
    assert_eq!(tak2["approvals"].as_array().unwrap().len(), 0, "izni olsa da kural rolu uyuşmaz → gormez");

    // Not: kural rolu olmayan akislarda submit dogrudan approved olur;
    // submitted yalnizca kurali adimlarda olusur — ustteki B senaryosu bunu kapsiyor.
}
