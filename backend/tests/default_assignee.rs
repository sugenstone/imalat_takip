//! Adima varsayilan atanan testleri: bir kez tanimla, her instance'da otomatik.

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

/// A (default: veli user) -> B akisi. Veli Worker.
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
    let worker_id = roles.as_array().unwrap().iter()
        .find(|r| r["name"] == "Worker").map(|r| r["id"].as_str().unwrap().to_string()).unwrap();
    ali.send(&app.app, "POST", &format!("/api/workspaces/{wid}/members"),
        Some(json!({ "email": "veli@t.com", "role_id": worker_id }))).await;

    let (_, blok) = ali.send(&app.app, "POST", &format!("/api/workspaces/{wid}/sections"),
        Some(json!({ "name": "Blok" }))).await;
    let (_, item) = ali.send(&app.app, "POST", &format!("/api/workspaces/{wid}/work-items"),
        Some(json!({ "section_id": blok["id"], "name": "Is Kalemi" }))).await;
    let iid = item["id"].as_str().unwrap().to_string();

    // Akis: A (default veli) -> B
    let (_, wf) = ali.send(&app.app, "POST", &format!("/api/workspaces/{wid}/workflows"),
        Some(json!({ "name": "Akis" }))).await;
    let tfid = wf["id"].as_str().unwrap().to_string();
    let (_, na) = ali.send(&app.app, "POST",
        &format!("/api/workspaces/{wid}/workflows/{tfid}/draft/nodes"),
        Some(json!({ "name": "Kesim" }))).await;
    let (_, nb) = ali.send(&app.app, "POST",
        &format!("/api/workspaces/{wid}/workflows/{tfid}/draft/nodes"),
        Some(json!({ "name": "Montaj" }))).await;
    ali.send(&app.app, "POST",
        &format!("/api/workspaces/{wid}/workflows/{tfid}/draft/dependencies"),
        Some(json!({ "predecessor_node_id": na["id"], "successor_node_id": nb["id"] }))).await;

    // Kesim adimina varsayilan atanan: veli (user)
    let (st, _) = ali.send(&app.app, "PATCH",
        &format!("/api/workspaces/{wid}/workflows/{tfid}/draft/nodes/{}", na["id"].as_str().unwrap()),
        Some(json!({ "default_assignee": { "type": "user", "id": veli_uid } }))).await;
    assert_eq!(st, StatusCode::OK, "default assignee set edilebilmeli");

    ali.send(&app.app, "POST", &format!("/api/workspaces/{wid}/workflows/{tfid}/publish"), None).await;

    (app, ali, veli, wid, iid, veli_uid, na["id"].as_str().unwrap().to_string(), tfid)
}

fn parse_assignments(v: &Value, name: &str) -> Vec<Value> {
    let p = v["processes"].as_array().unwrap().iter().find(|p| p["name"] == name).unwrap();
    serde_json::from_str::<Vec<Value>>(p["assignments"].as_str().unwrap_or("[]")).unwrap()
}

#[tokio::test]
async fn default_assignee_auto_assigned_with_notification() {
    let (app, mut ali, mut veli, wid, iid, _vuid, _na, _tfid) = bootstrap().await;

    // Akisi ata -> Kesim otomatik veli'ye atanmis olmali
    let (st, inst) = ali.send(&app.app, "POST",
        &format!("/api/workspaces/{wid}/work-items/{iid}/workflow"),
        Some(json!({ "template_id": _tfid }))).await;
    assert_eq!(st, StatusCode::CREATED);

    let kesim_assigns = parse_assignments(&inst, "Kesim");
    assert_eq!(kesim_assigns.len(), 1, "default atanan otomatik baglanmali");
    assert_eq!(kesim_assigns[0]["type"], "user");
    assert_eq!(kesim_assigns[0]["name"], "Veli");

    // Montaj'da default yok -> atama bos
    assert_eq!(parse_assignments(&inst, "Montaj").len(), 0);

    // Veli'ye bildirim dusmus olmali
    let (_, cnt) = veli.send(&app.app, "GET",
        &format!("/api/workspaces/{wid}/notifications/unread"), None).await;
    assert_eq!(cnt["unread"], 1, "default atanan bildirimi gonderilmeli");
    let (_, list) = veli.send(&app.app, "GET",
        &format!("/api/workspaces/{wid}/notifications"), None).await;
    assert_eq!(list[0]["type"], "process.assigned");
    assert!(list[0]["title"].as_str().unwrap().contains("Kesim"));
}

#[tokio::test]
async fn default_team_and_override() {
    let (app, mut ali, mut veli, wid, iid, veli_uid, _na, tfid) = bootstrap().await;

    // v2 draft BOS (publish v1 sonrasi yeni draft) — adimlari yeniden kur
    let (_, team) = ali.send(&app.app, "POST", &format!("/api/workspaces/{wid}/teams"),
        Some(json!({ "name": "Kesim Ekibi" }))).await;
    ali.send(&app.app, "POST",
        &format!("/api/workspaces/{wid}/teams/{}/members", team["id"].as_str().unwrap()),
        Some(json!({ "user_id": veli_uid }))).await;

    let (_, n2a) = ali.send(&app.app, "POST",
        &format!("/api/workspaces/{wid}/workflows/{tfid}/draft/nodes"),
        Some(json!({ "name": "Kesim" }))).await;
    let kesim_id = n2a["id"].as_str().unwrap().to_string();
    let (_, n2b) = ali.send(&app.app, "POST",
        &format!("/api/workspaces/{wid}/workflows/{tfid}/draft/nodes"),
        Some(json!({ "name": "Montaj" }))).await;
    let montaj_id = n2b["id"].as_str().unwrap().to_string();
    ali.send(&app.app, "POST",
        &format!("/api/workspaces/{wid}/workflows/{tfid}/draft/dependencies"),
        Some(json!({ "predecessor_node_id": kesim_id, "successor_node_id": montaj_id }))).await;

    let (st, _) = ali.send(&app.app, "PATCH",
        &format!("/api/workspaces/{wid}/workflows/{tfid}/draft/nodes/{kesim_id}"),
        Some(json!({ "default_assignee": { "type": "team", "id": team["id"] } }))).await;
    assert_eq!(st, StatusCode::OK);

    // Gecersiz assignee reddedilir
    let (st, _) = ali.send(&app.app, "PATCH",
        &format!("/api/workspaces/{wid}/workflows/{tfid}/draft/nodes/{kesim_id}"),
        Some(json!({ "default_assignee": { "type": "user", "id": "yok" } }))).await;
    assert_eq!(st, StatusCode::BAD_REQUEST);

    ali.send(&app.app, "POST", &format!("/api/workspaces/{wid}/workflows/{tfid}/publish"), None).await;

    // Yeni is kalemi + v3 ata -> Kesim takima atanir, takim uyesi bildirim alir
    let (_, secs) = ali.send(&app.app, "GET",
        &format!("/api/workspaces/{wid}/sections"), None).await;
    let (_, item2) = ali.send(&app.app, "POST", &format!("/api/workspaces/{wid}/work-items"),
        Some(json!({ "section_id": secs[0]["id"], "name": "Ikinci Is" }))).await;
    let iid2 = item2["id"].as_str().unwrap().to_string();
    ali.send(&app.app, "POST", &format!("/api/workspaces/{wid}/work-items/{iid2}/workflow"),
        Some(json!({ "template_id": tfid }))).await;

    let (_, inst2) = ali.send(&app.app, "GET",
        &format!("/api/workspaces/{wid}/work-items/{iid2}/workflow"), None).await;
    let assigns = parse_assignments(&inst2, "Kesim");
    assert_eq!(assigns.len(), 1);
    assert_eq!(assigns[0]["type"], "team");
    assert_eq!(assigns[0]["name"], "Kesim Ekibi");

    // Takim uyesi (veli) bildirim aldi
    let (_, cnt) = veli.send(&app.app, "GET",
        &format!("/api/workspaces/{wid}/notifications/unread"), None).await;
    assert!(cnt["unread"].as_i64().unwrap() >= 1, "takim uyesi bildirim almali");

    // OVERRIDE: default atamayi kaldir, baska uye ata
    let kesim_pid = inst2["processes"].as_array().unwrap().iter()
        .find(|p| p["name"] == "Kesim").map(|p| p["id"].as_str().unwrap().to_string()).unwrap();
    let assign_id = assigns[0]["id"].as_str().unwrap().to_string();
    let (st, _) = ali.send(&app.app, "DELETE",
        &format!("/api/workspaces/{wid}/process-instances/{kesim_pid}/assignments/{assign_id}"), None).await;
    assert_eq!(st, StatusCode::OK);

    // ali'nin user id'si ile yeni atama
    let (_, me_ali) = ali.send(&app.app, "GET", "/api/auth/me", None).await;
    let (st, _) = ali.send(&app.app, "POST",
        &format!("/api/workspaces/{wid}/process-instances/{kesim_pid}/assignments"),
        Some(json!({ "assignee_type": "user", "assignee_id": me_ali["id"] }))).await;
    assert_eq!(st, StatusCode::CREATED);

    let (_, inst3) = ali.send(&app.app, "GET",
        &format!("/api/workspaces/{wid}/work-items/{iid2}/workflow"), None).await;
    let assigns3 = parse_assignments(&inst3, "Kesim");
    assert_eq!(assigns3.len(), 1);
    assert_eq!(assigns3[0]["name"], "Ali", "override sonrasi yalniz yeni atama kalmali");

    let _ = iid;
    let _ = montaj_id;
}

#[allow(dead_code)]
fn _removed_tail_marker() {}
