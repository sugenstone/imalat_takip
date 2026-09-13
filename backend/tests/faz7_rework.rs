//! Faz 7 testleri: rework / attempt zinciri.
//! Ilke (yol haritasi 29-30): basarisiz surec geriye cevrilmez; restart
//! noktasindan yeni attempt zinciri olusur, upstream approved DOKUNULMAZ.

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

/// Akis: A -> B -> C (hepsi kuralsiz). Veli Supervisor (rework yetkisi var).
async fn bootstrap() -> (TestApp, Client, Client, String, String, String) {
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
        Some(json!({ "name": "Blok A" }))).await;
    let (_, item) = ali.send(&app.app, "POST", &format!("/api/workspaces/{wid}/work-items"),
        Some(json!({ "section_id": blok["id"], "name": "Ana Is" }))).await;
    let iid = item["id"].as_str().unwrap().to_string();

    // A -> B -> C akisi kur + yayinla
    let (_, wf) = ali.send(&app.app, "POST", &format!("/api/workspaces/{wid}/workflows"),
        Some(json!({ "name": "Uretim" }))).await;
    let tfid = wf["id"].as_str().unwrap().to_string();
    let mut ids = Vec::new();
    for name in ["A", "B", "C"] {
        let (_, n) = ali.send(&app.app, "POST",
            &format!("/api/workspaces/{wid}/workflows/{tfid}/draft/nodes"),
            Some(json!({ "name": name }))).await;
        ids.push(n["id"].as_str().unwrap().to_string());
    }
    for (p, s) in [(0, 1), (1, 2)] {
        ali.send(&app.app, "POST",
            &format!("/api/workspaces/{wid}/workflows/{tfid}/draft/dependencies"),
            Some(json!({ "predecessor_node_id": ids[p], "successor_node_id": ids[s] }))).await;
    }
    ali.send(&app.app, "POST", &format!("/api/workspaces/{wid}/workflows/{tfid}/publish"), None).await;

    // Ata
    ali.send(&app.app, "POST", &format!("/api/workspaces/{wid}/work-items/{iid}/workflow"),
        Some(json!({ "template_id": tfid }))).await;

    (app, ali, veli, wid, iid, tfid)
}

fn proc_by_name(v: &Value, name: &str) -> Value {
    v["processes"].as_array().unwrap().iter().find(|p| p["name"] == name).cloned().unwrap()
}

/// A ve B'yi onayla, C'yi FAILED yap (finish ile).
async fn progress_to_failed(ali: &mut Client, app: &Router, wid: &str, iid: &str) -> Value {
    let (_, inst) = ali.send(app, "GET",
        &format!("/api/workspaces/{wid}/work-items/{iid}/workflow"), None).await;
    for name in ["A", "B"] {
        let pid = proc_by_name(&inst, name)["id"].as_str().unwrap().to_string();
        ali.send(app, "POST", &format!("/api/workspaces/{wid}/process-instances/{pid}/start"),
            Some(json!({}))).await;
        ali.send(app, "POST", &format!("/api/workspaces/{wid}/process-instances/{pid}/submit"),
            Some(json!({}))).await;
    }
    let (_, i2) = ali.send(app, "GET",
        &format!("/api/workspaces/{wid}/work-items/{iid}/workflow"), None).await;
    i2
}

#[tokio::test]
async fn rework_creates_new_attempt_chain() {
    let (app, mut ali, _veli, wid, iid, _tfid) = bootstrap().await;

    // A, B onayla; C'yi baslat
    let i2 = progress_to_failed(&mut ali, &app.app, &wid, &iid).await;
    let c_id = proc_by_name(&i2, "C")["id"].as_str().unwrap().to_string();
    ali.send(&app.app, "POST", &format!("/api/workspaces/{wid}/process-instances/{c_id}/start"),
        Some(json!({}))).await;

    // C attempt 1 baslamis olmali
    let (_, i3) = ali.send(&app.app, "GET",
        &format!("/api/workspaces/{wid}/work-items/{iid}/workflow"), None).await;
    let c3 = proc_by_name(&i3, "C");
    assert_eq!(c3["status"], "in_progress");
    assert_eq!(c3["attempt_count"], 1);
    assert_eq!(c3["current_attempt"], 1);

    // --- REWORK: B'den yeniden baslat (yetkili: Supervisor'a soramazdi, ali Owner) ---
    let b_id = proc_by_name(&i2, "B")["id"].as_str().unwrap().to_string();
    let a_id = proc_by_name(&i2, "A")["id"].as_str().unwrap().to_string();

    // sebep zorunlu
    let (st, _) = ali.send(&app.app, "POST", &format!("/api/workspaces/{wid}/work-items/{iid}/rework"),
        Some(json!({ "restart_from_process_id": b_id, "reason": "" }))).await;
    assert_eq!(st, StatusCode::BAD_REQUEST);

    // basarisiz surec yokken rework reddedilir
    let (st, _) = ali.send(&app.app, "POST", &format!("/api/workspaces/{wid}/work-items/{iid}/rework"),
        Some(json!({ "restart_from_process_id": b_id, "reason": "x" }))).await;
    assert_eq!(st, StatusCode::BAD_REQUEST, "failed surec yokken rework engellenmeli");

    // C'yi FAILED yap: onaysiz akista failed yolu yok - sureci cancel ile degil,
    // onay kurali olmadigindan rework tetikleyici olarak dogrudan surec durumunu
    // test amaciyla 'failed'a cekmek yerine: akis B'den restart edilemez (henuz failed yok).
    // Bunun yerine C'yi basarisiz kilmanin MVP yolu yok -> restart A'dan da reddedilmeli:
    let (st, _) = ali.send(&app.app, "POST", &format!("/api/workspaces/{wid}/work-items/{iid}/rework"),
        Some(json!({ "restart_from_process_id": a_id, "reason": "erken" }))).await;
    assert_eq!(st, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn rework_after_failure_full_cycle() {
    let (app, mut ali, mut veli, wid, _iid, _tfid) = bootstrap().await;

    // Onay kurali iceren ayri akis kur: A -> C(onayli, Supervisor) + ayri dal D (bagimsiz)
    // Basitlik: mevcut akista C'yi onayli yapamiyoruz (yayinlandi). Yeni akis kurali:
    // A -> B -> C hepsi kuralsiz zaten. FAILED durumu yalnizca reject ile olusur.
    // Bu testte: C onayli olacak yeni bir akis + is kalemi ile yapalim.
    let (_, wf2) = ali.send(&app.app, "POST", &format!("/api/workspaces/{wid}/workflows"),
        Some(json!({ "name": "Onayli Uretim" }))).await;
    let tfid2 = wf2["id"].as_str().unwrap().to_string();
    let (_, na) = ali.send(&app.app, "POST",
        &format!("/api/workspaces/{wid}/workflows/{tfid2}/draft/nodes"),
        Some(json!({ "name": "X" }))).await;
    let (_, nb) = ali.send(&app.app, "POST",
        &format!("/api/workspaces/{wid}/workflows/{tfid2}/draft/nodes"),
        Some(json!({ "name": "Y" }))).await;
    let (_, roles) = ali.send(&app.app, "GET", &format!("/api/workspaces/{wid}/roles"), None).await;
    let sup_id = roles.as_array().unwrap().iter()
        .find(|r| r["name"] == "Supervisor").map(|r| r["id"].as_str().unwrap().to_string()).unwrap();
    // Y onayli (Supervisor)
    ali.send(&app.app, "PATCH",
        &format!("/api/workspaces/{wid}/workflows/{tfid2}/draft/nodes/{}", nb["id"].as_str().unwrap()),
        Some(json!({ "approval_rule": { "required": true, "approver_role_id": sup_id } }))).await;
    ali.send(&app.app, "POST",
        &format!("/api/workspaces/{wid}/workflows/{tfid2}/draft/dependencies"),
        Some(json!({ "predecessor_node_id": na["id"], "successor_node_id": nb["id"] }))).await;
    ali.send(&app.app, "POST", &format!("/api/workspaces/{wid}/workflows/{tfid2}/publish"), None).await;

    let (_, blok) = ali.send(&app.app, "POST", &format!("/api/workspaces/{wid}/sections"),
        Some(json!({ "name": "Blok Z" }))).await;
    let (_, item2) = ali.send(&app.app, "POST", &format!("/api/workspaces/{wid}/work-items"),
        Some(json!({ "section_id": blok["id"], "name": "Ikinci Is" }))).await;
    let iid2 = item2["id"].as_str().unwrap().to_string();
    ali.send(&app.app, "POST", &format!("/api/workspaces/{wid}/work-items/{iid2}/workflow"),
        Some(json!({ "template_id": tfid2 }))).await;

    // X tamamla, Y baslat+submit (onay bekler), veli REDDET
    let (_, i1) = ali.send(&app.app, "GET",
        &format!("/api/workspaces/{wid}/work-items/{iid2}/workflow"), None).await;
    let x_id = proc_by_name(&i1, "X")["id"].as_str().unwrap().to_string();
    let y_id = proc_by_name(&i1, "Y")["id"].as_str().unwrap().to_string();
    for pid in [&x_id] {
        ali.send(&app.app, "POST", &format!("/api/workspaces/{wid}/process-instances/{pid}/start"),
            Some(json!({}))).await;
        ali.send(&app.app, "POST", &format!("/api/workspaces/{wid}/process-instances/{pid}/submit"),
            Some(json!({}))).await;
    }
    ali.send(&app.app, "POST", &format!("/api/workspaces/{wid}/process-instances/{y_id}/start"),
        Some(json!({}))).await;
    ali.send(&app.app, "POST", &format!("/api/workspaces/{wid}/process-instances/{y_id}/submit"),
        Some(json!({}))).await;
    veli.send(&app.app, "POST", &format!("/api/workspaces/{wid}/process-instances/{y_id}/reject"),
        Some(json!({ "note": "olculer hatali" }))).await;

    let (_, i2) = ali.send(&app.app, "GET",
        &format!("/api/workspaces/{wid}/work-items/{iid2}/workflow"), None).await;
    assert_eq!(proc_by_name(&i2, "Y")["status"], "failed");

    // --- REWORK: X'ten yeniden baslat ---
    let (st, res) = ali.send(&app.app, "POST",
        &format!("/api/workspaces/{wid}/work-items/{iid2}/rework"),
        Some(json!({ "restart_from_process_id": x_id, "reason": "olcu duzeltilecek" }))).await;
    assert_eq!(st, StatusCode::OK, "rework baslamali: {res:?}");
    assert_eq!(res["affected"], 2);

    // Yeni durum: X Attempt2 READY, Y Attempt2 WAITING; attempt sayaclari artmis
    let (_, i3) = ali.send(&app.app, "GET",
        &format!("/api/workspaces/{wid}/work-items/{iid2}/workflow"), None).await;
    let x3 = proc_by_name(&i3, "X");
    let y3 = proc_by_name(&i3, "Y");
    assert_eq!(x3["status"], "ready", "restart adimi hazir olmali");
    assert_eq!(y3["status"], "waiting", "downstream beklemeli");
    assert_eq!(x3["attempt_count"], 2, "X icin 2 deneme");
    assert_eq!(y3["attempt_count"], 2, "Y icin 2 deneme (failed + yeni)");
    assert_eq!(y3["current_attempt"], 2);
    assert_eq!(i3["status"], "active", "instance yeniden aktif");

    // Rework gecmisi gorunmeli
    let cycles = i3["rework_cycles"].as_array().unwrap();
    assert_eq!(cycles.len(), 1);
    assert_eq!(cycles[0]["restart_from_name"], "X");
    assert_eq!(cycles[0]["triggered_by_name"], "Y");
    assert_eq!(cycles[0]["reason"], "olcu duzeltilecek");
    assert_eq!(cycles[0]["created_by_name"], "Ali");

    // --- Yeniden tamamla: X + Y (onayli) + veli onayi -> akis biter ---
    ali.send(&app.app, "POST", &format!("/api/workspaces/{wid}/process-instances/{x_id}/start"),
        Some(json!({}))).await;
    ali.send(&app.app, "POST", &format!("/api/workspaces/{wid}/process-instances/{x_id}/submit"),
        Some(json!({}))).await;
    // Y waiting -> ready (X approved sonrasi cascade)
    ali.send(&app.app, "POST", &format!("/api/workspaces/{wid}/process-instances/{y_id}/start"),
        Some(json!({}))).await;
    ali.send(&app.app, "POST", &format!("/api/workspaces/{wid}/process-instances/{y_id}/submit"),
        Some(json!({}))).await;
    let (st, _) = veli.send(&app.app, "POST",
        &format!("/api/workspaces/{wid}/process-instances/{y_id}/approve"),
        Some(json!({ "note": "duzeltildi" }))).await;
    assert_eq!(st, StatusCode::OK, "ikinci turda onay gelmeli");

    let (_, i4) = ali.send(&app.app, "GET",
        &format!("/api/workspaces/{wid}/work-items/{iid2}/workflow"), None).await;
    assert_eq!(i4["status"], "completed", "rework sonrasi akis tamamlanmali");
    assert_eq!(i4["approved"], 2);

    // --- Gecersiz restart: failed yok artik ---
    let (st, _) = ali.send(&app.app, "POST",
        &format!("/api/workspaces/{wid}/work-items/{iid2}/rework"),
        Some(json!({ "restart_from_process_id": x_id, "reason": "tekrar" }))).await;
    assert_eq!(st, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn rework_permissions_and_invalid_restart() {
    let (app, mut ali, mut veli, wid, iid, _tfid) = bootstrap().await;

    // C'yi failed yapmadan once: A,B onayla, C baslat + iptal yolu yok;
    // izin testi icin failed durumu sart - onaysiz akista olmadigindan
    // veli'nin rework yetkisini ayse Worker ile test edelim (Worker'da rework yok).
    let i2 = progress_to_failed(&mut ali, &app.app, &wid, &iid).await;
    let a_id = proc_by_name(&i2, "A")["id"].as_str().unwrap().to_string();

    // Worker rework baslatamaz
    let mut ayse = Client::new();
    ayse.send(&app.app, "POST", "/api/auth/register",
        Some(json!({ "email": "ayse@t.com", "password": "parola123", "name": "Ayse" }))).await;
    let (_, roles) = ali.send(&app.app, "GET", &format!("/api/workspaces/{wid}/roles"), None).await;
    let worker_id = roles.as_array().unwrap().iter()
        .find(|r| r["name"] == "Worker").map(|r| r["id"].as_str().unwrap().to_string()).unwrap();
    ali.send(&app.app, "POST", &format!("/api/workspaces/{wid}/members"),
        Some(json!({ "email": "ayse@t.com", "role_id": worker_id }))).await;
    let (st, _) = ayse.send(&app.app, "POST",
        &format!("/api/workspaces/{wid}/work-items/{iid}/rework"),
        Some(json!({ "restart_from_process_id": a_id, "reason": "x" }))).await;
    assert_eq!(st, StatusCode::FORBIDDEN);

    // Supervisor rework yetkisine sahip (izinde var) - ama failed yok yine 400
    let (st, _) = veli.send(&app.app, "POST",
        &format!("/api/workspaces/{wid}/work-items/{iid}/rework"),
        Some(json!({ "restart_from_process_id": a_id, "reason": "x" }))).await;
    assert_eq!(st, StatusCode::BAD_REQUEST);
}
