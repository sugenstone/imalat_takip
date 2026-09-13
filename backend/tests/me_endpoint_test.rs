//! GET /workspaces/{wid}/me testleri: rol bazli UI icin rol + izin paketi.

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

/// Ali owner, Ayse worker.
async fn bootstrap() -> (TestApp, Client, Client, String) {
    let app = setup().await;
    let mut ali = Client::new();
    ali.send(&app.app, "POST", "/api/auth/register",
        Some(json!({ "email": "ali@t.com", "password": "parola123", "name": "Ali" }))).await;
    let mut ayse = Client::new();
    ayse.send(&app.app, "POST", "/api/auth/register",
        Some(json!({ "email": "ayse@t.com", "password": "parola123", "name": "Ayse" }))).await;

    let (_, ws) = ali.send(&app.app, "POST", "/api/workspaces",
        Some(json!({ "name": "Atolye" }))).await;
    let wid = ws["id"].as_str().unwrap().to_string();

    let (_, roles) = ali.send(&app.app, "GET", &format!("/api/workspaces/{wid}/roles"), None).await;
    let worker_id = roles.as_array().unwrap().iter()
        .find(|r| r["name"] == "Worker").map(|r| r["id"].as_str().unwrap().to_string()).unwrap();
    ali.send(&app.app, "POST", &format!("/api/workspaces/{wid}/members"),
        Some(json!({ "email": "ayse@t.com", "role_id": worker_id }))).await;

    (app, ali, ayse, wid)
}

#[tokio::test]
async fn me_returns_role_and_permissions() {
    let (app, mut ali, mut ayse, wid) = bootstrap().await;

    // Owner: tam paket
    let (st, me) = ali.send(&app.app, "GET", &format!("/api/workspaces/{wid}/me"), None).await;
    assert_eq!(st, StatusCode::OK, "{me:?}");
    assert_eq!(me["role_name"], "Owner");
    assert_eq!(me["is_owner"], true);
    let perms = me["permissions"].as_array().unwrap();
    for key in ["work_item.create", "user.invite", "role.manage", "section.create", "workflow.create"] {
        assert!(perms.iter().any(|p| p == key), "Owner izni eksik: {key}");
    }

    // Worker: kisitli paket — yonetim izinleri yok, saha izinleri var
    let (st, me_w) = ayse.send(&app.app, "GET", &format!("/api/workspaces/{wid}/me"), None).await;
    assert_eq!(st, StatusCode::OK, "{me_w:?}");
    assert_eq!(me_w["role_name"], "Worker");
    assert_eq!(me_w["is_owner"], false);
    let perms_w = me_w["permissions"].as_array().unwrap();
    for key in ["work_item.create", "user.invite", "role.manage", "section.create", "workflow.create", "workflow.assign"] {
        assert!(!perms_w.iter().any(|p| p == key), "Worker'da yonetim izni olmamali: {key}");
    }
    for key in ["process.start", "process.complete"] {
        assert!(perms_w.iter().any(|p| p == key), "Worker saha izni eksik: {key}");
    }

    // Uye olmayan: 403
    let mut dis = Client::new();
    dis.send(&app.app, "POST", "/api/auth/register",
        Some(json!({ "email": "dis@t.com", "password": "parola123", "name": "Dis" }))).await;
    let (st_d, _) = dis.send(&app.app, "GET", &format!("/api/workspaces/{wid}/me"), None).await;
    assert_eq!(st_d, StatusCode::FORBIDDEN);
}
