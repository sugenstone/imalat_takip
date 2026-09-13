//! Hizli akis (quick-flow) testleri: tek cagrida olustur + zincir + yayinla + ata.

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
async fn quick_flow_full() {
    let app = setup().await;
    let mut ali = Client::new();
    ali.send(&app.app, "POST", "/api/auth/register",
        Some(json!({ "email": "ali@t.com", "password": "parola123", "name": "Ali" }))).await;
    let (_, ws) = ali.send(&app.app, "POST", "/api/workspaces",
        Some(json!({ "name": "Atolye" }))).await;
    let wid = ws["id"].as_str().unwrap().to_string();
    let (_, blok) = ali.send(&app.app, "POST", &format!("/api/workspaces/{wid}/sections"),
        Some(json!({ "name": "Blok" }))).await;
    let (_, item) = ali.send(&app.app, "POST", &format!("/api/workspaces/{wid}/work-items"),
        Some(json!({ "section_id": blok["id"], "name": "Tezgah" }))).await;
    let iid = item["id"].as_str().unwrap().to_string();

    // Hizli akis: 3 adim, sonuncusu onayli, is kalemine ata
    let (st, res) = ali.send(&app.app, "POST", &format!("/api/workspaces/{wid}/quick-flow"),
        Some(json!({
            "name": "Tezgah Akisi",
            "steps": [
                { "name": "Kesim" },
                { "name": "Imalat" },
                { "name": "Sevkiyat", "requires_approval": true }
            ],
            "assign_to_item_id": iid
        }))).await;
    assert_eq!(st, StatusCode::OK, "quick-flow basarili: {res:?}");
    assert_eq!(res["published_version"], 1);
    assert_eq!(res["assigned"], true);

    // Is kaleminde akis atanmis mi — 3 surec zincir halinde
    let (_, inst) = ali.send(&app.app, "GET",
        &format!("/api/workspaces/{wid}/work-items/{iid}/workflow"), None).await;
    assert_eq!(inst["template_name"], "Tezgah Akisi");
    let procs = inst["processes"].as_array().unwrap();
    assert_eq!(procs.len(), 3);
    assert_eq!(procs[0]["name"], "Kesim");
    assert_eq!(procs[0]["status"], "ready", "ilk adim hazir olmali");
    assert_eq!(procs[1]["status"], "waiting");
    assert_eq!(procs[2]["status"], "waiting");

    // Zincir bagimlilik: Kesim'in pred'i yok, Imalat'in pred'i Kesim, Sevkiyat'in pred'i Imalat
    let preds1: Vec<Value> = serde_json::from_str(procs[1]["predecessor_ids"].as_str().unwrap()).unwrap();
    let preds2: Vec<Value> = serde_json::from_str(procs[2]["predecessor_ids"].as_str().unwrap()).unwrap();
    assert_eq!(preds1.len(), 1);
    assert_eq!(preds1[0], procs[0]["node_id"]);
    assert_eq!(preds2.len(), 1);
    assert_eq!(preds2[0], procs[1]["node_id"]);

    // Akis listede v1 yayinda gorunur
    let (_, flows) = ali.send(&app.app, "GET", &format!("/api/workspaces/{wid}/workflows"), None).await;
    let f = flows.as_array().unwrap().iter().find(|f| f["name"] == "Tezgah Akisi").unwrap();
    assert_eq!(f["published_version"], 1);
    assert_eq!(f["published_node_count"], 3);

    // Yeni bos draft (v2) hazir
    let (_, vers) = ali.send(&app.app, "GET",
        &format!("/api/workspaces/{wid}/workflows/{}/versions", res["template_id"].as_str().unwrap()), None).await;
    assert_eq!(vers.as_array().unwrap().len(), 2);

    // Onay kurali: Sevkiyat'ta submit -> onay bekler
    let pid0 = procs[0]["id"].as_str().unwrap().to_string();
    let pid1 = procs[1]["id"].as_str().unwrap().to_string();
    let pid2 = procs[2]["id"].as_str().unwrap().to_string();
    for pid in [&pid0, &pid1] {
        ali.send(&app.app, "POST", &format!("/api/workspaces/{wid}/process-instances/{pid}/start"),
            Some(json!({}))).await;
        ali.send(&app.app, "POST", &format!("/api/workspaces/{wid}/process-instances/{pid}/submit"),
            Some(json!({}))).await;
    }
    ali.send(&app.app, "POST", &format!("/api/workspaces/{wid}/process-instances/{pid2}/start"),
        Some(json!({}))).await;
    let (_, sub) = ali.send(&app.app, "POST", &format!("/api/workspaces/{wid}/process-instances/{pid2}/submit"),
        Some(json!({}))).await;
    assert_eq!(sub["status"], "submitted", "onayli adim submitted'da beklemeli");

    // Hata durumları: 0 adim, 21 adim, bos ad
    let (st, _) = ali.send(&app.app, "POST", &format!("/api/workspaces/{wid}/quick-flow"),
        Some(json!({ "name": "Bos", "steps": [] }))).await;
    assert_eq!(st, StatusCode::BAD_REQUEST);
    let (st, _) = ali.send(&app.app, "POST", &format!("/api/workspaces/{wid}/quick-flow"),
        Some(json!({ "name": "Cok", "steps": (0..21).map(|i| json!({ "name": format!("A{i}") })).collect::<Vec<_>>() }))).await;
    assert_eq!(st, StatusCode::BAD_REQUEST);
    let (st, _) = ali.send(&app.app, "POST", &format!("/api/workspaces/{wid}/quick-flow"),
        Some(json!({ "name": "BosAd", "steps": [json!({ "name": "  " })] }))).await;
    assert_eq!(st, StatusCode::BAD_REQUEST);

    // Ikinci hizli akis ayni is kalemine: aktif instance var -> assigned false
    let (st, res2) = ali.send(&app.app, "POST", &format!("/api/workspaces/{wid}/quick-flow"),
        Some(json!({
            "name": "Baska Akis",
            "steps": [json!({ "name": "Tek" })],
            "assign_to_item_id": iid
        }))).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(res2["assigned"], false, "aktif instance varken yeniden atanmamali");

    // Atama olmadan quick-flow da calisir
    let (st, res3) = ali.send(&app.app, "POST", &format!("/api/workspaces/{wid}/quick-flow"),
        Some(json!({ "name": "Serbest", "steps": [json!({ "name": "A" }), json!({ "name": "B" })] }))).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(res3["assigned"], false);
}
