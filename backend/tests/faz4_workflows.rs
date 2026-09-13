//! Faz 4 testleri: workflow template -> versiyon -> DAG bagimliliklar -> publish.

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

async fn bootstrap() -> (TestApp, Client, String) {
    let app = setup().await;
    let mut ali = Client::new();
    ali.send(&app.app, "POST", "/api/auth/register",
        Some(json!({ "email": "ali@t.com", "password": "parola123", "name": "Ali" }))).await;
    let (_, ws) = ali.send(&app.app, "POST", "/api/workspaces",
        Some(json!({ "name": "Atolye" }))).await;
    let wid = ws["id"].as_str().unwrap().to_string();
    (app, ali, wid)
}

#[tokio::test]
async fn workflow_designer_full_flow() {
    let (app, mut ali, wid) = bootstrap().await;

    // --- Template olustur ---
    let (st, wf) = ali.send(&app.app, "POST", &format!("/api/workspaces/{wid}/workflows"),
        Some(json!({ "name": "Standart Uretim", "description": "Ana uretis akisi" }))).await;
    assert_eq!(st, StatusCode::CREATED);
    let tfid = wf["id"].as_str().unwrap().to_string();

    // ayni isim cakisma
    let (st, _) = ali.send(&app.app, "POST", &format!("/api/workspaces/{wid}/workflows"),
        Some(json!({ "name": "Standart Uretim" }))).await;
    assert_eq!(st, StatusCode::CONFLICT);

    // --- Ilk erisimde bos draft olusur ---
    let (st, draft) = ali.send(&app.app, "GET",
        &format!("/api/workspaces/{wid}/workflows/{tfid}/draft"), None).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(draft["version_number"], 1, "ilk draft v1 olmali");
    assert_eq!(draft["nodes"].as_array().unwrap().len(), 0);

    // --- Bos akis yayinlanamaz ---
    let (st, _) = ali.send(&app.app, "POST",
        &format!("/api/workspaces/{wid}/workflows/{tfid}/publish"), None).await;
    assert_eq!(st, StatusCode::BAD_REQUEST);

    // --- Adimlar: Tas Alimi -> Kesim -> Imalat, paralel: Boya + Elektrik -> Montaj ---
    let mut node_ids = Vec::new();
    for name in ["Tas Alimi", "Kesim", "Imalat", "Boya", "Elektrik", "Montaj"] {
        let (st, node) = ali.send(&app.app, "POST",
            &format!("/api/workspaces/{wid}/workflows/{tfid}/draft/nodes"),
            Some(json!({ "name": name }))).await;
        assert_eq!(st, StatusCode::CREATED, "{name} eklenmeli");
        node_ids.push(node["id"].as_str().unwrap().to_string());
    }
    let [tas, kesim, imalat, boya, elektrik, montaj] = [&node_ids[0], &node_ids[1], &node_ids[2], &node_ids[3], &node_ids[4], &node_ids[5]];

    // --- Bagimliliklar (DAG) ---
    let deps = [
        (tas, kesim),          // Tas Alimi -> Kesim
        (kesim, imalat),       // Kesim -> Imalat
        (imalat, boya),        // Imalat -> Boya
        (imalat, elektrik),    // Imalat -> Elektrik (paralel dal)
        (boya, montaj),        // Boya -> Montaj
        (elektrik, montaj),    // Elektrik -> Montaj (birlesen)
    ];
    for (p, s) in deps {
        let (st, _) = ali.send(&app.app, "POST",
            &format!("/api/workspaces/{wid}/workflows/{tfid}/draft/dependencies"),
            Some(json!({ "predecessor_node_id": p, "successor_node_id": s }))).await;
        assert_eq!(st, StatusCode::CREATED);
    }

    // ayni bagimlilik tekrar -> conflict
    let (st, _) = ali.send(&app.app, "POST",
        &format!("/api/workspaces/{wid}/workflows/{tfid}/draft/dependencies"),
        Some(json!({ "predecessor_node_id": tas, "successor_node_id": kesim }))).await;
    assert_eq!(st, StatusCode::CONFLICT);

    // --- CYCLE KORUMASI (bagimlilik ekleme aninda) ---
    // Kesim -> Tas Alimi eklenmeye calilsin: Tas Alimi'dan Kesim'a zaten yol var -> cycle
    let (st, _) = ali.send(&app.app, "POST",
        &format!("/api/workspaces/{wid}/workflows/{tfid}/draft/dependencies"),
        Some(json!({ "predecessor_node_id": kesim, "successor_node_id": tas }))).await;
    assert_eq!(st, StatusCode::BAD_REQUEST, "cycle engellenmeli");

    // kendine bagimlilik
    let (st, _) = ali.send(&app.app, "POST",
        &format!("/api/workspaces/{wid}/workflows/{tfid}/draft/dependencies"),
        Some(json!({ "predecessor_node_id": montaj, "successor_node_id": montaj }))).await;
    assert_eq!(st, StatusCode::BAD_REQUEST);

    // baska draft'in node'u gecersiz
    let (_, wf2) = ali.send(&app.app, "POST", &format!("/api/workspaces/{wid}/workflows"),
        Some(json!({ "name": "Diger Akis" }))).await;
    let tfid2 = wf2["id"].as_str().unwrap().to_string();
    let (_, n2) = ali.send(&app.app, "POST",
        &format!("/api/workspaces/{wid}/workflows/{tfid2}/draft/nodes"),
        Some(json!({ "name": "Baska" }))).await;
    let (st, _) = ali.send(&app.app, "POST",
        &format!("/api/workspaces/{wid}/workflows/{tfid}/draft/dependencies"),
        Some(json!({ "predecessor_node_id": n2["id"], "successor_node_id": montaj }))).await;
    assert_eq!(st, StatusCode::BAD_REQUEST, "yabanci node reddedilmeli");

    // --- Draft dogrulama ---
    let (_, draft) = ali.send(&app.app, "GET",
        &format!("/api/workspaces/{wid}/workflows/{tfid}/draft"), None).await;
    assert_eq!(draft["nodes"].as_array().unwrap().len(), 6);
    assert_eq!(draft["dependencies"].as_array().unwrap().len(), 6);

    // --- PUBLISH ---
    let (st, res) = ali.send(&app.app, "POST",
        &format!("/api/workspaces/{wid}/workflows/{tfid}/publish"), None).await;
    assert_eq!(st, StatusCode::OK, "publish basarili olmali: {res:?}");
    assert_eq!(res["published_version"], 1);
    assert_eq!(res["next_draft_version"], 2);

    // Liste ozeti guncel
    let (_, list) = ali.send(&app.app, "GET",
        &format!("/api/workspaces/{wid}/workflows"), None).await;
    let w = list.as_array().unwrap().iter().find(|w| w["id"] == *tfid).unwrap();
    assert_eq!(w["published_version"], 1);
    assert_eq!(w["published_node_count"], 6);
    assert_eq!(w["draft_node_count"], 0, "yeni bos draft olusmali");

    // --- Versiyon gecmisi ---
    let (_, versions) = ali.send(&app.app, "GET",
        &format!("/api/workspaces/{wid}/workflows/{tfid}/versions"), None).await;
    let vs = versions.as_array().unwrap();
    assert_eq!(vs.len(), 2, "v1 published + v2 draft");
    assert_eq!(vs[0]["version_number"], 2); // DESC sirali
    assert_eq!(vs[0]["status"], "draft");
    assert_eq!(vs[1]["version_number"], 1);
    assert_eq!(vs[1]["status"], "published");
    assert_eq!(vs[1]["node_count"], 6);

    // --- v2 draft'ina yeni adim -> yeni publish ---
    let (st, _) = ali.send(&app.app, "POST",
        &format!("/api/workspaces/{wid}/workflows/{tfid}/draft/nodes"),
        Some(json!({ "name": "Paketleme" }))).await;
    assert_eq!(st, StatusCode::CREATED);

    let (st, res2) = ali.send(&app.app, "POST",
        &format!("/api/workspaces/{wid}/workflows/{tfid}/publish"), None).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(res2["published_version"], 2);
    assert_eq!(res2["next_draft_version"], 3);

    let (_, versions) = ali.send(&app.app, "GET",
        &format!("/api/workspaces/{wid}/workflows/{tfid}/versions"), None).await;
    assert_eq!(versions.as_array().unwrap().len(), 3, "v1+v2 published, v3 draft");

    // v1 hala duruyor ve degismedi (immutable)
    let v1 = versions.as_array().unwrap().iter().find(|v| v["version_number"] == 1).unwrap();
    assert_eq!(v1["node_count"], 6, "v1 immutable - 6 adim kalmali");
}

#[tokio::test]
async fn workflow_permissions_and_archive() {
    let (app, mut ali, wid) = bootstrap().await;

    // veli Viewer
    let mut veli = Client::new();
    veli.send(&app.app, "POST", "/api/auth/register",
        Some(json!({ "email": "veli@t.com", "password": "parola123", "name": "Veli" }))).await;
    let (_, roles) = ali.send(&app.app, "GET", &format!("/api/workspaces/{wid}/roles"), None).await;
    let viewer_id = roles.as_array().unwrap().iter()
        .find(|r| r["name"] == "Viewer").map(|r| r["id"].as_str().unwrap().to_string()).unwrap();
    ali.send(&app.app, "POST", &format!("/api/workspaces/{wid}/members"),
        Some(json!({ "email": "veli@t.com", "role_id": viewer_id }))).await;

    let (_, wf) = ali.send(&app.app, "POST", &format!("/api/workspaces/{wid}/workflows"),
        Some(json!({ "name": "Kalite Akisi" }))).await;
    let tfid = wf["id"].as_str().unwrap().to_string();

    // Viewer akis olusturamaz
    let (st, _) = veli.send(&app.app, "POST", &format!("/api/workspaces/{wid}/workflows"),
        Some(json!({ "name": "Izinsiz" }))).await;
    assert_eq!(st, StatusCode::FORBIDDEN);

    // ama goruntuleyebilir
    let (st, _) = veli.send(&app.app, "GET",
        &format!("/api/workspaces/{wid}/workflows/{tfid}/draft"), None).await;
    assert_eq!(st, StatusCode::OK);

    // node ekleme de yasak
    let (st, _) = veli.send(&app.app, "POST",
        &format!("/api/workspaces/{wid}/workflows/{tfid}/draft/nodes"),
        Some(json!({ "name": "X" }))).await;
    assert_eq!(st, StatusCode::FORBIDDEN);

    // --- node silme (bagimliliklariyla) ---
    let (_, n1) = ali.send(&app.app, "POST",
        &format!("/api/workspaces/{wid}/workflows/{tfid}/draft/nodes"),
        Some(json!({ "name": "A" }))).await;
    let (_, n2) = ali.send(&app.app, "POST",
        &format!("/api/workspaces/{wid}/workflows/{tfid}/draft/nodes"),
        Some(json!({ "name": "B" }))).await;
    ali.send(&app.app, "POST",
        &format!("/api/workspaces/{wid}/workflows/{tfid}/draft/dependencies"),
        Some(json!({ "predecessor_node_id": n1["id"], "successor_node_id": n2["id"] }))).await;

    // n1 silinince bagimlilik da gitmeli (ON DELETE CASCADE)
    let (st, _) = ali.send(&app.app, "DELETE",
        &format!("/api/workspaces/{wid}/workflows/{tfid}/draft/nodes/{}", n1["id"].as_str().unwrap()), None).await;
    assert_eq!(st, StatusCode::OK);

    let (_, draft) = ali.send(&app.app, "GET",
        &format!("/api/workspaces/{wid}/workflows/{tfid}/draft"), None).await;
    assert_eq!(draft["nodes"].as_array().unwrap().len(), 1);
    assert_eq!(draft["dependencies"].as_array().unwrap().len(), 0, "bagimlilik cascade silinmeli");

    // --- template arsivleme ---
    let (st, _) = ali.send(&app.app, "POST",
        &format!("/api/workspaces/{wid}/workflows/{tfid}/archive"), None).await;
    assert_eq!(st, StatusCode::OK);
    let (_, list) = ali.send(&app.app, "GET",
        &format!("/api/workspaces/{wid}/workflows"), None).await;
    assert_eq!(list.as_array().unwrap().len(), 0, "arsivlenen listeden dusmeli");
}
