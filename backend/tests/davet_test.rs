//! Davet sistemi testleri: olustur -> e-posta kuyrugu -> kabul -> uyelik + oturum.

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
async fn invite_full_flow() {
    let app = setup().await;
    let mut ali = Client::new();
    ali.send(&app.app, "POST", "/api/auth/register",
        Some(json!({ "email": "ali@t.com", "password": "parola123", "name": "Ali" }))).await;
    let (_, ws) = ali.send(&app.app, "POST", "/api/workspaces",
        Some(json!({ "name": "Atolye" }))).await;
    let wid = ws["id"].as_str().unwrap().to_string();

    let (_, roles) = ali.send(&app.app, "GET", &format!("/api/workspaces/{wid}/roles"), None).await;
    let worker_id = roles.as_array().unwrap().iter()
        .find(|r| r["name"] == "Worker").map(|r| r["id"].as_str().unwrap().to_string()).unwrap();

    // 1) Davet olustur
    let (st, inv) = ali.send(&app.app, "POST", &format!("/api/workspaces/{wid}/invites"),
        Some(json!({ "email": "veli@yeni.com", "role_id": worker_id }))).await;
    assert_eq!(st, StatusCode::CREATED, "davet olusmali: {inv:?}");
    let token = inv["token"].as_str().unwrap().to_string();

    // 2) Outbox'ta davet e-postasi (link token icerir)
    let (_, outbox) = ali.send(&app.app, "GET",
        &format!("/api/workspaces/{wid}/notifications/emails"), None).await;
    let mails = outbox.as_array().unwrap();
    assert!(!mails.is_empty(), "davet maili kuyrukta olmali");
    assert_eq!(mails[0]["to_email"], "veli@yeni.com");
    assert!(mails[0]["subject"].as_str().unwrap().contains("davet"));

    // 3) Ayni email'e ikinci bekleyen davet -> 409
    let (st, _) = ali.send(&app.app, "POST", &format!("/api/workspaces/{wid}/invites"),
        Some(json!({ "email": "veli@yeni.com", "role_id": worker_id }))).await;
    assert_eq!(st, StatusCode::CONFLICT);

    // 4) Public bilgi
    let (st, info) = Client::new().send(&app.app, "GET",
        &format!("/api/invites/{token}"), None).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(info["workspace_name"], "Atolye");
    assert_eq!(info["email"], "veli@yeni.com");
    assert_eq!(info["role_name"], "Worker");
    assert_eq!(info["existing_account"], false);

    // 5) Kabul: hesap olusur + uyelik + oturum
    let mut veli = Client::new();
    let (st, acc) = veli.send(&app.app, "POST",
        &format!("/api/invites/{token}/accept"),
        Some(json!({ "name": "Veli", "password": "parola123" }))).await;
    assert_eq!(st, StatusCode::OK, "kabul basarili olmali: {acc:?}");
    assert_eq!(acc["workspace_id"], wid);

    // Oturum acti mi (me calisir)?
    let (st, me) = veli.send(&app.app, "GET", "/api/auth/me", None).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(me["email"], "veli@yeni.com");

    // Workspace listesinde gorur
    let (_, wss) = veli.send(&app.app, "GET", "/api/workspaces", None).await;
    assert_eq!(wss.as_array().unwrap().len(), 1);
    assert_eq!(wss[0]["role_name"], "Worker");

    // 6) Token tek kullanim: tekrar kabul -> hata
    let (st, _) = Client::new().send(&app.app, "POST",
        &format!("/api/invites/{token}/accept"),
        Some(json!({ "name": "X", "password": "parola123" }))).await;
    assert_eq!(st, StatusCode::BAD_REQUEST);

    // 7) Yanlis token -> 404
    let (st, _) = Client::new().send(&app.app, "GET", "/api/invites/yokboyle", None).await;
    assert_eq!(st, StatusCode::NOT_FOUND);

    // 8) Zaten uye olan email'e davet -> 409
    let (st, _) = ali.send(&app.app, "POST", &format!("/api/workspaces/{wid}/invites"),
        Some(json!({ "email": "veli@yeni.com", "role_id": worker_id }))).await;
    assert_eq!(st, StatusCode::CONFLICT);

    // 9) Davet listesi: accepted gorunur
    let (_, invites) = ali.send(&app.app, "GET", &format!("/api/workspaces/{wid}/invites"), None).await;
    assert_eq!(invites.as_array().unwrap().len(), 1);
    assert_eq!(invites[0]["status"], "accepted");

    // 10) Members panelinde veli
    let (_, members) = ali.send(&app.app, "GET", &format!("/api/workspaces/{wid}/members"), None).await;
    let emails: Vec<&str> = members.as_array().unwrap().iter().map(|m| m["email"].as_str().unwrap()).collect();
    assert!(emails.contains(&"veli@yeni.com"));
}

#[tokio::test]
async fn invite_cancel_and_gecersiz_token() {
    let app = setup().await;
    let mut ali = Client::new();
    ali.send(&app.app, "POST", "/api/auth/register",
        Some(json!({ "email": "ali@t.com", "password": "parola123", "name": "Ali" }))).await;
    let (_, ws) = ali.send(&app.app, "POST", "/api/workspaces",
        Some(json!({ "name": "Atolye" }))).await;
    let wid = ws["id"].as_str().unwrap().to_string();
    let (_, roles) = ali.send(&app.app, "GET", &format!("/api/workspaces/{wid}/roles"), None).await;
    let viewer_id = roles.as_array().unwrap().iter()
        .find(|r| r["name"] == "Viewer").map(|r| r["id"].as_str().unwrap().to_string()).unwrap();

    // Davet olustur + iptal et
    let (_, inv) = ali.send(&app.app, "POST", &format!("/api/workspaces/{wid}/invites"),
        Some(json!({ "email": "ayse@yeni.com", "role_id": viewer_id }))).await;
    let iid = inv["id"].as_str().unwrap().to_string();
    let token = inv["token"].as_str().unwrap().to_string();

    let (st, _) = ali.send(&app.app, "DELETE",
        &format!("/api/workspaces/{wid}/invites/{iid}"), None).await;
    assert_eq!(st, StatusCode::OK);

    // Iptal edilen token ile kabul -> hata
    let (st, _) = Client::new().send(&app.app, "POST",
        &format!("/api/invites/{token}/accept"),
        Some(json!({ "name": "Ayse", "password": "parola123" }))).await;
    assert_eq!(st, StatusCode::BAD_REQUEST);

    // Tekrar iptal -> 404
    let (st, _) = ali.send(&app.app, "DELETE",
        &format!("/api/workspaces/{wid}/invites/{iid}"), None).await;
    assert_eq!(st, StatusCode::NOT_FOUND);

    // Gecersiz rol ile davet -> 400
    let (st, _) = ali.send(&app.app, "POST", &format!("/api/workspaces/{wid}/invites"),
        Some(json!({ "email": "x@y.com", "role_id": "yok" }))).await;
    assert_eq!(st, StatusCode::BAD_REQUEST);

    // Gecersiz e-posta
    let (st, _) = ali.send(&app.app, "POST", &format!("/api/workspaces/{wid}/invites"),
        Some(json!({ "email": "gecersiz", "role_id": viewer_id }))).await;
    assert_eq!(st, StatusCode::BAD_REQUEST);

    // Yeni davet: ayni email tekrar davet edilebilir (onceki iptal edildi)
    let (st, _) = ali.send(&app.app, "POST", &format!("/api/workspaces/{wid}/invites"),
        Some(json!({ "email": "ayse@yeni.com", "role_id": viewer_id }))).await;
    assert_eq!(st, StatusCode::CREATED);
}

#[tokio::test]
async fn invite_existing_user_with_password() {
    let app = setup().await;
    // iki kayitli kullanici
    let mut ali = Client::new();
    ali.send(&app.app, "POST", "/api/auth/register",
        Some(json!({ "email": "ali@t.com", "password": "parola123", "name": "Ali" }))).await;
    let mut mehmet = Client::new();
    mehmet.send(&app.app, "POST", "/api/auth/register",
        Some(json!({ "email": "mehmet@t.com", "password": "parola123", "name": "Mehmet" }))).await;

    let (_, ws) = ali.send(&app.app, "POST", "/api/workspaces",
        Some(json!({ "name": "Atolye" }))).await;
    let wid = ws["id"].as_str().unwrap().to_string();
    let (_, roles) = ali.send(&app.app, "GET", &format!("/api/workspaces/{wid}/roles"), None).await;
    let sup_id = roles.as_array().unwrap().iter()
        .find(|r| r["name"] == "Supervisor").map(|r| r["id"].as_str().unwrap().to_string()).unwrap();

    // Kayitli kullaniciya davet
    let (_, inv) = ali.send(&app.app, "POST", &format!("/api/workspaces/{wid}/invites"),
        Some(json!({ "email": "mehmet@t.com", "role_id": sup_id }))).await;
    let token = inv["token"].as_str().unwrap().to_string();

    // Bilgide existing_account true
    let (_, info) = Client::new().send(&app.app, "GET", &format!("/api/invites/{token}"), None).await;
    assert_eq!(info["existing_account"], true);

    // Yanlis sifre ile kabul -> hata
    let (st, _) = Client::new().send(&app.app, "POST",
        &format!("/api/invites/{token}/accept"),
        Some(json!({ "name": "Mehmet", "password": "yanlis" }))).await;
    assert_eq!(st, StatusCode::BAD_REQUEST);

    // Dogru sifre -> giris + uyelik
    let mut m2 = Client::new();
    let (st, _) = m2.send(&app.app, "POST",
        &format!("/api/invites/{token}/accept"),
        Some(json!({ "name": "Mehmet", "password": "parola123" }))).await;
    assert_eq!(st, StatusCode::OK);
    let (_, wss) = m2.send(&app.app, "GET", "/api/workspaces", None).await;
    assert_eq!(wss[0]["role_name"], "Supervisor");
}
