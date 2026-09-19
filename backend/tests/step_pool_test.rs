//! Adim Havuzu + Surec Grubu testleri:
//! havuz CRUD/izin -> gruba kopyalanan varsayilanlar -> dagitimda grup atama -> isci gorevi.

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

/// Ali owner, Veli worker (uid'li).
async fn bootstrap() -> (TestApp, Client, Client, String, String) {
    let app = setup().await;
    let mut ali = Client::new();
    ali.send(&app.app, "POST", "/api/auth/register",
        Some(json!({ "email": "ali@t.com", "password": "parola123", "name": "Ali" }))).await;
    let mut veli = Client::new();
    veli.send(&app.app, "POST", "/api/auth/register",
        Some(json!({ "email": "veli@t.com", "password": "parola123", "name": "Veli" }))).await;
    let (_, me_v) = veli.send(&app.app, "GET", "/api/auth/me", None).await;
    let veli_uid = me_v["id"].as_str().unwrap().to_string();

    let (_, ws) = ali.send(&app.app, "POST", "/api/workspaces",
        Some(json!({ "name": "Atolye" }))).await;
    let wid = ws["id"].as_str().unwrap().to_string();

    let (_, roles) = ali.send(&app.app, "GET", &format!("/api/workspaces/{wid}/roles"), None).await;
    let worker_id = roles.as_array().unwrap().iter()
        .find(|r| r["name"] == "Worker").map(|r| r["id"].as_str().unwrap().to_string()).unwrap();
    ali.send(&app.app, "POST", &format!("/api/workspaces/{wid}/members"),
        Some(json!({ "email": "veli@t.com", "role_id": worker_id }))).await;

    (app, ali, veli, wid, veli_uid)
}

#[tokio::test]
async fn step_pool_crud_and_permissions() {
    let (app, mut ali, mut veli, wid, veli_uid) = bootstrap().await;

    // Worker havuza adim ekleyemez
    let (st, _) = veli.send(&app.app, "POST", &format!("/api/workspaces/{wid}/steps"),
        Some(json!({ "name": "Kesim" }))).await;
    assert_eq!(st, StatusCode::FORBIDDEN);

    // Olustur: varsayilan sorumlu Veli + onay
    let (st, step1) = ali.send(&app.app, "POST", &format!("/api/workspaces/{wid}/steps"),
        Some(json!({
            "name": "Kesim",
            "default_assignee": { "type": "user", "id": veli_uid },
            "requires_approval": false
        }))).await;
    assert_eq!(st, StatusCode::CREATED, "{step1:?}");
    assert_eq!(step1["default_assignee_type"], "user");
    assert_eq!(step1["default_assignee_id"], veli_uid);

    let (st, step2) = ali.send(&app.app, "POST", &format!("/api/workspaces/{wid}/steps"),
        Some(json!({ "name": "Kontrol", "requires_approval": true }))).await;
    assert_eq!(st, StatusCode::CREATED);

    // Ayni ad tekrar -> 409
    let (st, _) = ali.send(&app.app, "POST", &format!("/api/workspaces/{wid}/steps"),
        Some(json!({ "name": "Kesim" }))).await;
    assert_eq!(st, StatusCode::CONFLICT);

    // Liste sirali
    let (_, list) = ali.send(&app.app, "GET", &format!("/api/workspaces/{wid}/steps"), None).await;
    assert_eq!(list.as_array().unwrap().len(), 2);

    // Guncelle
    let sid2 = step2["id"].as_str().unwrap().to_string();
    let (st, _) = ali.send(&app.app, "PATCH", &format!("/api/workspaces/{wid}/steps/{sid2}"),
        Some(json!({ "name": "Final Kontrol" }))).await;
    assert_eq!(st, StatusCode::OK);

    // Gecersiz sorumlu -> 400
    let (st, _) = ali.send(&app.app, "PATCH", &format!("/api/workspaces/{wid}/steps/{sid2}"),
        Some(json!({ "default_assignee": { "type": "user", "id": "yok" } }))).await;
    assert_eq!(st, StatusCode::BAD_REQUEST);

    // Arsivle -> listeden duser
    let (st, _) = ali.send(&app.app, "POST", &format!("/api/workspaces/{wid}/steps/{sid2}/archive"), None).await;
    assert_eq!(st, StatusCode::OK);
    let (_, list2) = ali.send(&app.app, "GET", &format!("/api/workspaces/{wid}/steps"), None).await;
    assert_eq!(list2.as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn step_group_to_distribute_end_to_end() {
    let (app, mut ali, mut veli, wid, veli_uid) = bootstrap().await;

    // Havuz: Kesim (Veli sorumlu) -> Kontrol (onayli)
    let (_, kesim) = ali.send(&app.app, "POST", &format!("/api/workspaces/{wid}/steps"),
        Some(json!({ "name": "Kesim", "default_assignee": { "type": "user", "id": veli_uid } }))).await;
    let (_, kontrol) = ali.send(&app.app, "POST", &format!("/api/workspaces/{wid}/steps"),
        Some(json!({ "name": "Kontrol", "requires_approval": true }))).await;

    // Surec grubu kur: Kesim -> Kontrol
    let (st, group) = ali.send(&app.app, "POST", &format!("/api/workspaces/{wid}/step-groups"),
        Some(json!({
            "name": "Mutfak Tezgahi Grubu",
            "step_ids": [ kesim["id"], kontrol["id"] ]
        }))).await;
    assert_eq!(st, StatusCode::OK, "{group:?}");
    assert_eq!(group["published_version"], 1);
    let gid = group["template_id"].as_str().unwrap().to_string();

    // Ayni ad tekrar secilemez
    let (st, _) = ali.send(&app.app, "POST", &format!("/api/workspaces/{wid}/step-groups"),
        Some(json!({ "name": "G2", "step_ids": [ kesim["id"], kesim["id"] ] }))).await;
    assert_eq!(st, StatusCode::BAD_REQUEST);

    // Bolum agaci: Kok -> 2 cocuk (yaprak)
    let (_, root) = ali.send(&app.app, "POST", &format!("/api/workspaces/{wid}/sections"),
        Some(json!({ "name": "Bina" }))).await;
    let root_id = root["id"].as_str().unwrap().to_string();
    for n in ["Daire 1", "Daire 2"] {
        ali.send(&app.app, "POST", &format!("/api/workspaces/{wid}/sections"),
            Some(json!({ "name": n, "parent_id": root_id }))).await;
    }

    // DAGIT: her yapraga kalem + surec grubu
    let (st, res) = ali.send(&app.app, "POST", &format!("/api/workspaces/{wid}/work-items/bulk-distribute"),
        Some(json!({
            "parent_section_id": root_id,
            "target": "leaves",
            "name": "{parent} Tezgahi",
            "workflow_template_id": gid
        }))).await;
    assert_eq!(st, StatusCode::OK, "{res:?}");
    assert_eq!(res["created"], 2);
    assert_eq!(res["auto_assigned"], 2, "grup her kaleme atanmali");

    // Kalemler: aktif + akista 2 adim; Kesim Veli'ye atanmis
    let (_, items) = ali.send(&app.app, "GET",
        &format!("/api/workspaces/{wid}/work-items?section_id={root_id}&subtree=true"), None).await;
    for it in items.as_array().unwrap() {
        assert_eq!(it["status"], "active", "grup atanan kalem aktif olmali");
        let iid = it["id"].as_str().unwrap().to_string();
        let (_, inst) = ali.send(&app.app, "GET",
            &format!("/api/workspaces/{wid}/work-items/{iid}/workflow"), None).await;
        assert_eq!(inst["status"], "active");
        let procs = inst["processes"].as_array().unwrap();
        assert_eq!(procs.len(), 2);
        assert_eq!(procs[0]["name"], "Kesim");
        assert_eq!(procs[0]["status"], "ready");
        let assigns: Vec<Value> = serde_json::from_str(procs[0]["assignments"].as_str().unwrap()).unwrap();
        assert_eq!(assigns.len(), 1, "Kesim havuzdaki varsayilan sorumluya atanmali");
        assert_eq!(assigns[0]["assignee_id"], veli_uid);
        assert_eq!(procs[1]["name"], "Kontrol");
        assert_eq!(procs[1]["status"], "waiting");
    }

    // Isci: Gorevlerim'de Kesim hazir
    let (_, tasks) = veli.send(&app.app, "GET", &format!("/api/workspaces/{wid}/my-tasks"), None).await;
    assert_eq!(tasks["ready"].as_array().unwrap().len(), 2, "Veli iki dairede Kesim gorevi gormeli");
    assert_eq!(tasks["ready"][0]["step_name"], "Kesim");
}
