//! Gercek senaryo paketi testleri: ic ice seri + dagitim + otomatik akis.

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

async fn mk_flow(
    ali: &mut Client,
    app: &Router,
    wid: &str,
    name: &str,
    steps: &[&str],
) -> String {
    let (_, wf) = ali.send(app, "POST", &format!("/api/workspaces/{wid}/workflows"),
        Some(json!({ "name": name }))).await;
    let tfid = wf["id"].as_str().unwrap().to_string();
    for st_ in steps {
        ali.send(app, "POST",
            &format!("/api/workspaces/{wid}/workflows/{tfid}/draft/nodes"),
            Some(json!({ "name": st_ }))).await;
    }
    let (_, draft) = ali.send(app, "GET",
        &format!("/api/workspaces/{wid}/workflows/{tfid}/draft"), None).await;
    let nodes = draft["nodes"].as_array().unwrap();
    for i in 0..steps.len().saturating_sub(1) {
        ali.send(app, "POST",
            &format!("/api/workspaces/{wid}/workflows/{tfid}/draft/dependencies"),
            Some(json!({ "predecessor_node_id": nodes[i]["id"], "successor_node_id": nodes[i+1]["id"] }))).await;
    }
    ali.send(app, "POST", &format!("/api/workspaces/{wid}/workflows/{tfid}/publish"), None).await;
    tfid
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
async fn nested_serial_continue_and_restart() {
    let (app, mut ali, wid) = bootstrap().await;

    // Bina koku + ic ice seri: 3 Kat x 4 Daire (continue)
    let (_, bina) = ali.send(&app.app, "POST", &format!("/api/workspaces/{wid}/sections"),
        Some(json!({ "name": "Bina" }))).await;
    let (st, res) = ali.send(&app.app, "POST",
        &format!("/api/workspaces/{wid}/sections/serial-nested"),
        Some(json!({
            "parent_id": bina["id"],
            "outer": { "template": "Kat {n}", "start": 1, "end": 3 },
            "inner": { "template": "Daire {n}", "count": 4, "numbering": "continue" }
        }))).await;
    assert_eq!(st, StatusCode::OK, "nested seri olusmali: {res:?}");
    assert_eq!(res["created"], 15); // 3 kat + 12 daire

    let (_, secs) = ali.send(&app.app, "GET", &format!("/api/workspaces/{wid}/sections"), None).await;
    let names: Vec<String> = secs.as_array().unwrap().iter()
        .map(|s| s["name"].as_str().unwrap().to_string()).collect();

    // Katlar
    for k in 1..=3 {
        assert!(names.contains(&format!("Kat {k}")), "Kat {k} olmali: {names:?}");
    }
    // CONTINUE: daire numaralari global — Kat1: 1-4, Kat2: 5-8, Kat3: 9-12
    for n in 1..=12 {
        assert!(names.contains(&format!("Daire {n}")), "Daire {n} olmali (continue): {names:?}");
    }
    // Kat 2'nin cocuklari 5-8 olmali (depth kontrolu)
    let kat2 = secs.as_array().unwrap().iter().find(|s| s["name"] == "Kat 2").unwrap();
    assert_eq!(kat2["depth"], 2);
    let daire5 = secs.as_array().unwrap().iter().find(|s| s["name"] == "Daire 5").unwrap();
    assert_eq!(daire5["depth"], 3);

    // RESTART modu: ayri kokte her grupta 1-4
    let (_, kok2) = ali.send(&app.app, "POST", &format!("/api/workspaces/{wid}/sections"),
        Some(json!({ "name": "Site B" }))).await;
    let (st, _) = ali.send(&app.app, "POST",
        &format!("/api/workspaces/{wid}/sections/serial-nested"),
        Some(json!({
            "parent_id": kok2["id"],
            "outer": { "template": "Blok {n}", "start": 1, "end": 2 },
            "inner": { "template": "Daire {n}", "count": 3, "numbering": "restart" }
        }))).await;
    assert_eq!(st, StatusCode::OK);

    let (_, secs2) = ali.send(&app.app, "GET", &format!("/api/workspaces/{wid}/sections"), None).await;
    let names2: Vec<&str> = secs2.as_array().unwrap().iter()
        .map(|s| s["name"].as_str().unwrap()).collect();
    // restart: iki blokta da Daire 1-3; Bina'daki continue Daire 1 ile birlikte toplam 3
    let d1_count = names2.iter().filter(|n| **n == "Daire 1").count();
    assert_eq!(d1_count, 3, "restart: 2 yeni + 1 continue = 3 Daire 1");

    // limit asimi: 100 x 100 > 500
    let (st, _) = ali.send(&app.app, "POST",
        &format!("/api/workspaces/{wid}/sections/serial-nested"),
        Some(json!({
            "outer": { "template": "X {n}", "start": 1, "end": 100 },
            "inner": { "template": "Y {n}", "count": 100 }
        }))).await;
    assert_eq!(st, StatusCode::BAD_REQUEST);

    // token yoksa reddedilir
    let (st, _) = ali.send(&app.app, "POST",
        &format!("/api/workspaces/{wid}/sections/serial-nested"),
        Some(json!({
            "outer": { "template": "Kat", "start": 1, "end": 2 },
            "inner": { "template": "Daire {n}", "count": 2 }
        }))).await;
    assert_eq!(st, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn distribute_with_auto_workflow() {
    let (app, mut ali, wid) = bootstrap().await;

    // Yapi: Bina -> 2 Kat x 2 Daire
    let (_, bina) = ali.send(&app.app, "POST", &format!("/api/workspaces/{wid}/sections"),
        Some(json!({ "name": "Bina" }))).await;
    ali.send(&app.app, "POST",
        &format!("/api/workspaces/{wid}/sections/serial-nested"),
        Some(json!({
            "parent_id": bina["id"],
            "outer": { "template": "Kat {n}", "start": 1, "end": 2 },
            "inner": { "template": "Daire {n}", "count": 2 }
        }))).await;

    // Iki ayri akis: Tezgah (Kesim->Imalat->Sevkiyat), Tezgah Arasi (Kesim->Sevkiyat)
    let tezgah_flow = mk_flow(&mut ali, &app.app, &wid, "Tezgah Akisi", &["Kesim", "Imalat", "Sevkiyat"]).await;
    let arasi_flow = mk_flow(&mut ali, &app.app, &wid, "Tezgah Arasi Akisi", &["Kesim", "Sevkiyat"]).await;

    // Iki is tipi + varsayilan akis bagla
    let (_, wt1) = ali.send(&app.app, "POST", &format!("/api/workspaces/{wid}/work-types"),
        Some(json!({ "name": "Tezgah" }))).await;
    let (_, wt2) = ali.send(&app.app, "POST", &format!("/api/workspaces/{wid}/work-types"),
        Some(json!({ "name": "Tezgah Arasi" }))).await;
    let t1 = wt1["id"].as_str().unwrap().to_string();
    let t2 = wt2["id"].as_str().unwrap().to_string();
    let (st, _) = ali.send(&app.app, "PATCH", &format!("/api/workspaces/{wid}/work-types/{t1}"),
        Some(json!({ "name": "Tezgah", "default_workflow_template_id": tezgah_flow }))).await;
    assert_eq!(st, StatusCode::OK);
    ali.send(&app.app, "PATCH", &format!("/api/workspaces/{wid}/work-types/{t2}"),
        Some(json!({ "name": "Tezgah Arasi", "default_workflow_template_id": arasi_flow }))).await;

    // Gecersiz akis baglanamaz
    let (st, _) = ali.send(&app.app, "PATCH", &format!("/api/workspaces/{wid}/work-types/{t1}"),
        Some(json!({ "name": "Tezgah", "default_workflow_template_id": "yok" }))).await;
    assert_eq!(st, StatusCode::BAD_REQUEST);
    // geri dogrusunu bagla
    ali.send(&app.app, "PATCH", &format!("/api/workspaces/{wid}/work-types/{t1}"),
        Some(json!({ "name": "Tezgah", "default_workflow_template_id": tezgah_flow }))).await;

    // DAGITIM 1: her daireye "Mutfak Tezgahi" (tezgah tipi, otomatik akis)
    let (st, res) = ali.send(&app.app, "POST",
        &format!("/api/workspaces/{wid}/work-items/bulk-distribute"),
        Some(json!({
            "parent_section_id": bina["id"],
            "target": "leaves",
            "work_type_id": t1,
            "name": "Mutfak Tezgahı",
            "auto_workflow": true,
            "status": "active"
        }))).await;
    assert_eq!(st, StatusCode::OK, "dagitim basarili: {res:?}");
    assert_eq!(res["created"], 4, "4 daireye 4 is kalemi");
    assert_eq!(res["auto_assigned"], 4, "4 akis otomatik atanmali");

    // DAGITIM 2: Tezgah Arasi ({parent} tokenu ile)
    let (st, res2) = ali.send(&app.app, "POST",
        &format!("/api/workspaces/{wid}/work-items/bulk-distribute"),
        Some(json!({
            "parent_section_id": bina["id"],
            "target": "leaves",
            "work_type_id": t2,
            "name": "{parent} Tezgah Arası",
            "auto_workflow": true,
            "status": "active"
        }))).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(res2["created"], 4);
    assert_eq!(res2["auto_assigned"], 4);

    // Toplam 8 is kalemi
    let (_, items) = ali.send(&app.app, "GET", &format!("/api/workspaces/{wid}/work-items"), None).await;
    assert_eq!(items.as_array().unwrap().len(), 8);

    // {parent} tokenu: "Daire 1 Tezgah Arasi" olmali
    let names: Vec<&str> = items.as_array().unwrap().iter().map(|i| i["name"].as_str().unwrap()).collect();
    assert!(names.contains(&"Daire 1 Tezgah Arası"), "{names:?}");

    // FARKLI SURECLER: tezgah tipindeki is kaleminde 3 adim, arasi tipinde 2 adim
    let tezgah_item = items.as_array().unwrap().iter().find(|i| i["name"] == "Mutfak Tezgahı").unwrap();
    let arasi_item = items.as_array().unwrap().iter().find(|i| i["name"] == "Daire 1 Tezgah Arası").unwrap();

    let (_, w1) = ali.send(&app.app, "GET",
        &format!("/api/workspaces/{wid}/work-items/{}/workflow", tezgah_item["id"].as_str().unwrap()), None).await;
    assert_eq!(w1["processes"].as_array().unwrap().len(), 3, "Tezgah akisi 3 adim");
    assert_eq!(w1["template_name"], "Tezgah Akisi");
    // kok adim Hazir
    assert_eq!(w1["processes"][0]["status"], "ready");

    let (_, w2) = ali.send(&app.app, "GET",
        &format!("/api/workspaces/{wid}/work-items/{}/workflow", arasi_item["id"].as_str().unwrap()), None).await;
    assert_eq!(w2["processes"].as_array().unwrap().len(), 2, "Tezgah Arasi akisi 2 adim (Imalat yok)");
    assert_eq!(w2["template_name"], "Tezgah Arasi Akisi");

    // children hedefi: Katlar'a dagit (2 kat) — dairelere degil
    let (st, res3) = ali.send(&app.app, "POST",
        &format!("/api/workspaces/{wid}/work-items/bulk-distribute"),
        Some(json!({
            "parent_section_id": bina["id"],
            "target": "children",
            "work_type_id": t1,
            "name": "Kat Gorevi"
        }))).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(res3["created"], 2, "dogrudan cocuklar: 2 kat");
    assert_eq!(res3["auto_assigned"], 0, "auto_workflow kapaliydi");

    // tek create + auto_workflow
    let (_, secs) = ali.send(&app.app, "GET", &format!("/api/workspaces/{wid}/sections"), None).await;
    let daire1 = secs.as_array().unwrap().iter().find(|s| s["name"] == "Daire 1").unwrap();
    let (st, item) = ali.send(&app.app, "POST", &format!("/api/workspaces/{wid}/work-items"),
        Some(json!({
            "section_id": daire1["id"],
            "work_type_id": t1,
            "name": "Balkon Tezgahı",
            "auto_workflow": true
        }))).await;
    assert_eq!(st, StatusCode::CREATED);
    let (_, w3) = ali.send(&app.app, "GET",
        &format!("/api/workspaces/{wid}/work-items/{}/workflow", item["id"].as_str().unwrap()), None).await;
    assert_eq!(w3["processes"].as_array().unwrap().len(), 3, "tek create'te otomatik akis atanmali");
}
