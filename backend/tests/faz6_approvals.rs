//! Faz 6 testleri: atama + onay katmani (submit != approve, rol kisitli onay, red, bana atananlar).

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

/// Ortam: A -> B (B onay gerektirir, Supervisor rolu) akisi + is kalemi.
/// Veli Supervisor olarak ekli.
async fn bootstrap() -> (TestApp, Client, Client, String, String, String, Value, Value, String) {
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

    // Veli Supervisor
    let (_, roles) = ali.send(&app.app, "GET", &format!("/api/workspaces/{wid}/roles"), None).await;
    let sup_id = roles.as_array().unwrap().iter()
        .find(|r| r["name"] == "Supervisor").map(|r| r["id"].as_str().unwrap().to_string()).unwrap();
    ali.send(&app.app, "POST", &format!("/api/workspaces/{wid}/members"),
        Some(json!({ "email": "veli@t.com", "role_id": sup_id }))).await;

    // Veli user id
    let (_, me) = veli.send(&app.app, "GET", "/api/auth/me", None).await;
    let veli_uid = me["id"].as_str().unwrap().to_string();

    let (_, blok) = ali.send(&app.app, "POST", &format!("/api/workspaces/{wid}/sections"),
        Some(json!({ "name": "Blok A" }))).await;
    let (_, item) = ali.send(&app.app, "POST", &format!("/api/workspaces/{wid}/work-items"),
        Some(json!({ "section_id": blok["id"], "name": "Ana Is" }))).await;
    let iid = item["id"].as_str().unwrap().to_string();

    // Akis: A -> B
    let (_, wf) = ali.send(&app.app, "POST", &format!("/api/workspaces/{wid}/workflows"),
        Some(json!({ "name": "Onayli Akis" }))).await;
    let tfid = wf["id"].as_str().unwrap().to_string();
    let (_, na) = ali.send(&app.app, "POST",
        &format!("/api/workspaces/{wid}/workflows/{tfid}/draft/nodes"),
        Some(json!({ "name": "A" }))).await;
    let (_, nb) = ali.send(&app.app, "POST",
        &format!("/api/workspaces/{wid}/workflows/{tfid}/draft/nodes"),
        Some(json!({ "name": "B" }))).await;
    ali.send(&app.app, "POST",
        &format!("/api/workspaces/{wid}/workflows/{tfid}/draft/dependencies"),
        Some(json!({ "predecessor_node_id": na["id"], "successor_node_id": nb["id"] }))).await;

    // B'ye onay kurali: Supervisor onaylamali
    ali.send(&app.app, "PATCH",
        &format!("/api/workspaces/{wid}/workflows/{tfid}/draft/nodes/{}", nb["id"].as_str().unwrap()),
        Some(json!({ "approval_rule": { "required": true, "approver_role_id": sup_id } }))).await;

    ali.send(&app.app, "POST",
        &format!("/api/workspaces/{wid}/workflows/{tfid}/publish"), None).await;

    (app, ali, veli, wid, iid, tfid, na, nb, veli_uid)
}

fn parse_json_val(v: &Value, key: &str) -> Value {
    match v[key].as_str() {
        Some(s) => serde_json::from_str(s).unwrap_or(Value::Null),
        None => Value::Null,
    }
}

fn proc_by_name(v: &Value, name: &str) -> Value {
    v["processes"].as_array().unwrap().iter().find(|p| p["name"] == name).cloned().unwrap()
}

#[tokio::test]
async fn approval_flow_submit_approve_reject() {
    let (app, mut ali, mut veli, wid, iid, tfid, _na, _nb, _veli_uid) = bootstrap().await;

    // Ata + A'yi tamamla (kuralsiz: direkt approved)
    let (_, inst) = ali.send(&app.app, "POST",
        &format!("/api/workspaces/{wid}/work-items/{iid}/workflow"),
        Some(json!({ "template_id": tfid }))).await;
    let a_id = proc_by_name(&inst, "A")["id"].as_str().unwrap().to_string();
    let b_id = proc_by_name(&inst, "B")["id"].as_str().unwrap().to_string();

    ali.send(&app.app, "POST", &format!("/api/workspaces/{wid}/process-instances/{a_id}/start"),
        Some(json!({}))).await;
    let (st, res) = ali.send(&app.app, "POST",
        &format!("/api/workspaces/{wid}/process-instances/{a_id}/submit"),
        Some(json!({}))).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(res["status"], "approved", "kuralsiz node direkt onaylanmali");

    // B ready olmali
    let (_, i2) = ali.send(&app.app, "GET",
        &format!("/api/workspaces/{wid}/work-items/{iid}/workflow"), None).await;
    assert_eq!(proc_by_name(&i2, "B")["status"], "ready");

    // B'yi baslat + submit -> ONAY BEKLER
    ali.send(&app.app, "POST", &format!("/api/workspaces/{wid}/process-instances/{b_id}/start"),
        Some(json!({}))).await;
    let (st, res) = ali.send(&app.app, "POST",
        &format!("/api/workspaces/{wid}/process-instances/{b_id}/submit"),
        Some(json!({}))).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(res["status"], "submitted", "kurali node onay beklemeli");

    let (_, i3) = ali.send(&app.app, "GET",
        &format!("/api/workspaces/{wid}/work-items/{iid}/workflow"), None).await;
    let b3 = proc_by_name(&i3, "B");
    assert_eq!(b3["status"], "submitted");
    let approval = parse_json_val(&b3, "approval");
    assert_eq!(approval["decision"], "pending");
    assert_eq!(approval["requested_by_name"], "Ali");

    // submitted iken tekrar submit edilemez
    let (st, _) = ali.send(&app.app, "POST",
        &format!("/api/workspaces/{wid}/process-instances/{b_id}/submit"),
        Some(json!({}))).await;
    assert_eq!(st, StatusCode::BAD_REQUEST);

    // Worker (onay yetkisi yok) onaylayamaz - yeni worker kullanici
    let mut ayse = Client::new();
    ayse.send(&app.app, "POST", "/api/auth/register",
        Some(json!({ "email": "ayse@t.com", "password": "parola123", "name": "Ayse" }))).await;
    let (_, roles) = ali.send(&app.app, "GET", &format!("/api/workspaces/{wid}/roles"), None).await;
    let worker_id = roles.as_array().unwrap().iter()
        .find(|r| r["name"] == "Worker").map(|r| r["id"].as_str().unwrap().to_string()).unwrap();
    ali.send(&app.app, "POST", &format!("/api/workspaces/{wid}/members"),
        Some(json!({ "email": "ayse@t.com", "role_id": worker_id }))).await;
    let (st, _) = ayse.send(&app.app, "POST",
        &format!("/api/workspaces/{wid}/process-instances/{b_id}/approve"),
        Some(json!({}))).await;
    assert_eq!(st, StatusCode::FORBIDDEN, "Worker onaylayamaz");

    // Veli (Supervisor) REDDER - not zorunlu
    let (st, _) = veli.send(&app.app, "POST",
        &format!("/api/workspaces/{wid}/process-instances/{b_id}/reject"),
        Some(json!({}))).await;
    assert_eq!(st, StatusCode::BAD_REQUEST, "redsiz not engellenmeli");

    let (st, _) = veli.send(&app.app, "POST",
        &format!("/api/workspaces/{wid}/process-instances/{b_id}/reject"),
        Some(json!({ "note": "Yuzey hatali" }))).await;
    assert_eq!(st, StatusCode::OK, "Supervisor reddedebilmeli: onay kurali rolu");

    let (_, i4) = ali.send(&app.app, "GET",
        &format!("/api/workspaces/{wid}/work-items/{iid}/workflow"), None).await;
    let b4 = proc_by_name(&i4, "B");
    assert_eq!(b4["status"], "failed");
    let ap4 = parse_json_val(&b4, "approval");
    assert_eq!(ap4["decision"], "rejected");
    assert_eq!(ap4["note"], "Yuzey hatali");
    assert_eq!(ap4["decided_by_name"], "Veli");

    // Reddedilen surec tekrar onaylanamaz
    let (st, _) = veli.send(&app.app, "POST",
        &format!("/api/workspaces/{wid}/process-instances/{b_id}/approve"),
        Some(json!({}))).await;
    assert_eq!(st, StatusCode::BAD_REQUEST);

    // --- Yeni atama ile onayli senaryo (approve yolu) ---
    let (_, item2) = ali.send(&app.app, "POST", &format!("/api/workspaces/{wid}/work-items"),
        Some(json!({ "section_id": proc_by_name(&i4, "A").get("x"), "name": "X" }))).await;
    let _ = item2; // section_id gecersiz olabilir - atlamistik, asagida duz akis
}

#[tokio::test]
async fn approve_cascade_and_assignments() {
    let (app, mut ali, mut veli, wid, iid, tfid, _na, _nb, veli_uid) = bootstrap().await;

    // Takim kur + veli'yi uye yap
    let (_, team) = ali.send(&app.app, "POST", &format!("/api/workspaces/{wid}/teams"),
        Some(json!({ "name": "Kalite Ekibi" }))).await;
    ali.send(&app.app, "POST",
        &format!("/api/workspaces/{wid}/teams/{}/members", team["id"].as_str().unwrap()),
        Some(json!({ "user_id": veli_uid }))).await;

    // Ata
    let (_, inst) = ali.send(&app.app, "POST",
        &format!("/api/workspaces/{wid}/work-items/{iid}/workflow"),
        Some(json!({ "template_id": tfid }))).await;
    let a_id = proc_by_name(&inst, "A")["id"].as_str().unwrap().to_string();

    // A'ya user olarak veli'yi ata
    let (st, _) = ali.send(&app.app, "POST",
        &format!("/api/workspaces/{wid}/process-instances/{a_id}/assignments"),
        Some(json!({ "assignee_type": "user", "assignee_id": veli_uid }))).await;
    assert_eq!(st, StatusCode::CREATED);

    // ayni atama tekrar -> 409
    let (st, _) = ali.send(&app.app, "POST",
        &format!("/api/workspaces/{wid}/process-instances/{a_id}/assignments"),
        Some(json!({ "assignee_type": "user", "assignee_id": veli_uid }))).await;
    assert_eq!(st, StatusCode::CONFLICT);

    // Takim atamasi
    let (st, _) = ali.send(&app.app, "POST",
        &format!("/api/workspaces/{wid}/process-instances/{a_id}/assignments"),
        Some(json!({ "assignee_type": "team", "assignee_id": team["id"] }))).await;
    assert_eq!(st, StatusCode::CREATED);

    // Gecersiz assignee
    let (st, _) = ali.send(&app.app, "POST",
        &format!("/api/workspaces/{wid}/process-instances/{a_id}/assignments"),
        Some(json!({ "assignee_type": "user", "assignee_id": "yok" }))).await;
    assert_eq!(st, StatusCode::BAD_REQUEST);
    let (st, _) = ali.send(&app.app, "POST",
        &format!("/api/workspaces/{wid}/process-instances/{a_id}/assignments"),
        Some(json!({ "assignee_type": "rol", "assignee_id": "x" }))).await;
    assert_eq!(st, StatusCode::BAD_REQUEST);

    // Atamalar goruntude
    let (_, i1) = ali.send(&app.app, "GET",
        &format!("/api/workspaces/{wid}/work-items/{iid}/workflow"), None).await;
    let a1 = proc_by_name(&i1, "A");
    let assigns: Vec<Value> = serde_json::from_str(a1["assignments"].as_str().unwrap()).unwrap();
    assert_eq!(assigns.len(), 2);
    let names: Vec<&str> = assigns.iter().map(|a| a["name"].as_str().unwrap()).collect();
    assert!(names.contains(&"Veli"));
    assert!(names.contains(&"Kalite Ekibi"));

    // BANA ATANANLAR filtresi: veli user VE takim uyesi olarak gorur
    let (st, mine) = veli.send(&app.app, "GET",
        &format!("/api/workspaces/{wid}/work-items?mine=true"), None).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(mine.as_array().unwrap().len(), 1, "veli bana atananlarda gormeli");

    // ali (atanmamis) gormez
    let (_, mine_ali) = ali.send(&app.app, "GET",
        &format!("/api/workspaces/{wid}/work-items?mine=true"), None).await;
    assert_eq!(mine_ali.as_array().unwrap().len(), 0);

    // A'yi tamamla, B submit -> ONAY BEKLEYEN filtresi
    ali.send(&app.app, "POST", &format!("/api/workspaces/{wid}/process-instances/{a_id}/start"),
        Some(json!({}))).await;
    ali.send(&app.app, "POST", &format!("/api/workspaces/{wid}/process-instances/{a_id}/submit"),
        Some(json!({}))).await;
    let (_, i2) = ali.send(&app.app, "GET",
        &format!("/api/workspaces/{wid}/work-items/{iid}/workflow"), None).await;
    let b_id = proc_by_name(&i2, "B")["id"].as_str().unwrap().to_string();
    ali.send(&app.app, "POST", &format!("/api/workspaces/{wid}/process-instances/{b_id}/start"),
        Some(json!({}))).await;
    ali.send(&app.app, "POST", &format!("/api/workspaces/{wid}/process-instances/{b_id}/submit"),
        Some(json!({}))).await;

    let (_, pend) = veli.send(&app.app, "GET",
        &format!("/api/workspaces/{wid}/work-items?pending_approval=true"), None).await;
    assert_eq!(pend.as_array().unwrap().len(), 1, "onay bekleyen listede olmali");

    // Supervisor ONAYLAR -> akis tamamlanir
    let (st, _) = veli.send(&app.app, "POST",
        &format!("/api/workspaces/{wid}/process-instances/{b_id}/approve"),
        Some(json!({ "note": "Uygun" }))).await;
    assert_eq!(st, StatusCode::OK, "Supervisor onaylayabilmeli");

    let (_, i3) = ali.send(&app.app, "GET",
        &format!("/api/workspaces/{wid}/work-items/{iid}/workflow"), None).await;
    assert_eq!(i3["status"], "completed", "onay sonrasi akis tamamlanmali");
    let b3 = proc_by_name(&i3, "B");
    let ap3 = parse_json_val(&b3, "approval");
    assert_eq!(ap3["decision"], "approved");
    assert_eq!(ap3["decided_by_name"], "Veli");

    // Onay bekleyen artik bos
    let (_, pend2) = veli.send(&app.app, "GET",
        &format!("/api/workspaces/{wid}/work-items?pending_approval=true"), None).await;
    assert_eq!(pend2.as_array().unwrap().len(), 0);

    // Atama kaldirma (gecmis korunur)
    let assign_id = assigns[0]["id"].as_str().unwrap().to_string();
    let (st, _) = ali.send(&app.app, "DELETE",
        &format!("/api/workspaces/{wid}/process-instances/{a_id}/assignments/{assign_id}"), None).await;
    assert_eq!(st, StatusCode::OK);
    let (_, i4) = ali.send(&app.app, "GET",
        &format!("/api/workspaces/{wid}/work-items/{iid}/workflow"), None).await;
    let a4 = proc_by_name(&i4, "A");
    let assigns4: Vec<Value> = serde_json::from_str(a4["assignments"].as_str().unwrap()).unwrap();
    assert_eq!(assigns4.len(), 1, "kaldirilan atama aktif listeden dusmeli");
}
