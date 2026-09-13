//! Faz 3 testleri: is tipleri, dinamik ozellikler, is kalemleri.

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

/// Hazir ortam: ali + workspace + bolum agaci dondurur.
async fn bootstrap() -> (TestApp, Client, String, String, String) {
    let app = setup().await;
    let mut ali = Client::new();
    ali.send(&app.app, "POST", "/api/auth/register",
        Some(json!({ "email": "ali@t.com", "password": "parola123", "name": "Ali" }))).await;

    let (_, ws) = ali.send(&app.app, "POST", "/api/workspaces",
        Some(json!({ "name": "Atolye" }))).await;
    let wid = ws["id"].as_str().unwrap().to_string();

    let (_, blok) = ali.send(&app.app, "POST", &format!("/api/workspaces/{wid}/sections"),
        Some(json!({ "name": "Blok A" }))).await;
    let blok_id = blok["id"].as_str().unwrap().to_string();

    (app, ali, wid, blok_id, String::new())
}

#[tokio::test]
async fn work_type_attributes_and_items_flow() {
    let (app, mut ali, wid, blok_id, _) = bootstrap().await;

    // --- Is tipi olustur ---
    let (st, wt) = ali.send(&app.app, "POST", &format!("/api/workspaces/{wid}/work-types"),
        Some(json!({ "name": "Tezgah", "description": "Mutfak tezgahi" }))).await;
    assert_eq!(st, StatusCode::CREATED);
    let tid = wt["id"].as_str().unwrap().to_string();

    // --- Ozellik tanimlari (12 tipten karisik) ---
    let (st, _) = ali.send(&app.app, "POST",
        &format!("/api/workspaces/{wid}/work-types/{tid}/attributes"),
        Some(json!({ "name": "Metraj", "data_type": "decimal", "unit": "m2", "is_required": true, "is_filterable": true }))).await;
    assert_eq!(st, StatusCode::CREATED);

    let (st, malzeme) = ali.send(&app.app, "POST",
        &format!("/api/workspaces/{wid}/work-types/{tid}/attributes"),
        Some(json!({ "name": "Malzeme", "data_type": "select", "is_filterable": true,
                     "options": ["Laminat", "Granit", "Mernet"] }))).await;
    assert_eq!(st, StatusCode::CREATED);
    let malzeme_id = malzeme["id"].as_str().unwrap().to_string();

    let (st, _) = ali.send(&app.app, "POST",
        &format!("/api/workspaces/{wid}/work-types/{tid}/attributes"),
        Some(json!({ "name": "Etiketler", "data_type": "multiselect",
                     "options": ["Acil", "Dis Kaynak", "Faturalandi"] }))).await;
    assert_eq!(st, StatusCode::CREATED);

    let (st, _) = ali.send(&app.app, "POST",
        &format!("/api/workspaces/{wid}/work-types/{tid}/attributes"),
        Some(json!({ "name": "Termin", "data_type": "date" }))).await;
    assert_eq!(st, StatusCode::CREATED);

    // gecersiz veri tipi reddedilir
    let (st, _) = ali.send(&app.app, "POST",
        &format!("/api/workspaces/{wid}/work-types/{tid}/attributes"),
        Some(json!({ "name": "X", "data_type": "unknown" }))).await;
    assert_eq!(st, StatusCode::BAD_REQUEST);

    // seceneksiz select reddedilir
    let (st, _) = ali.send(&app.app, "POST",
        &format!("/api/workspaces/{wid}/work-types/{tid}/attributes"),
        Some(json!({ "name": "Y", "data_type": "select", "options": [] }))).await;
    assert_eq!(st, StatusCode::BAD_REQUEST);

    // tanimlari listele
    let (st, defs) = ali.send(&app.app, "GET",
        &format!("/api/workspaces/{wid}/work-types/{tid}/attributes"), None).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(defs.as_array().unwrap().len(), 4);
    let metraj_id = defs.as_array().unwrap().iter()
        .find(|d| d["name"] == "Metraj").map(|d| d["id"].as_str().unwrap().to_string()).unwrap();
    let etiket_id = defs.as_array().unwrap().iter()
        .find(|d| d["name"] == "Etiketler").map(|d| d["id"].as_str().unwrap().to_string()).unwrap();

    // --- Is kalemi olustur (degerlerle) ---
    let (st, item) = ali.send(&app.app, "POST", &format!("/api/workspaces/{wid}/work-items"),
        Some(json!({
            "section_id": blok_id,
            "work_type_id": tid,
            "name": "Tezgah 1",
            "priority": "high",
            "status": "active",
            "attributes": [
                { "definition_id": metraj_id, "value": 3.5 },
                { "definition_id": malzeme_id, "value": "Granit" },
                { "definition_id": etiket_id, "value": ["Acil", "Dis Kaynak"] }
            ]
        }))).await;
    assert_eq!(st, StatusCode::CREATED, "is kalemi olusmali: {item:?}");
    let iid = item["id"].as_str().unwrap().to_string();

    // zorunlu alan (Metraj) eksikken olusmaz
    let (st, _) = ali.send(&app.app, "POST", &format!("/api/workspaces/{wid}/work-items"),
        Some(json!({
            "section_id": blok_id,
            "work_type_id": tid,
            "name": "Eksik",
            "attributes": []
        }))).await;
    assert_eq!(st, StatusCode::BAD_REQUEST, "zorunlu ozellik engellenmeli");

    // gecersiz secenek degeri reddedilir
    let (st, _) = ali.send(&app.app, "POST", &format!("/api/workspaces/{wid}/work-items"),
        Some(json!({
            "section_id": blok_id,
            "work_type_id": tid,
            "name": "Bozuk",
            "attributes": [
                { "definition_id": metraj_id, "value": 1 },
                { "definition_id": malzeme_id, "value": "Mevcut Olmayan" }
            ]
        }))).await;
    assert_eq!(st, StatusCode::BAD_REQUEST);

    // --- Detay: ozellik degerleri tipli donmeli ---
    let (st, det) = ali.send(&app.app, "GET", &format!("/api/workspaces/{wid}/work-items/{iid}"), None).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(det["name"], "Tezgah 1");
    assert_eq!(det["section_path"][0], "Blok A");
    let attrs = det["attributes"].as_array().unwrap();
    assert_eq!(attrs.len(), 4);
    let m = attrs.iter().find(|a| a["name"] == "Metraj").unwrap();
    assert_eq!(m["value"], json!(3.5));
    let mz = attrs.iter().find(|a| a["name"] == "Malzeme").unwrap();
    assert_eq!(mz["value"], json!("Granit"));
    let e = attrs.iter().find(|a| a["name"] == "Etiketler").unwrap();
    assert_eq!(e["value"], json!(["Acil", "Dis Kaynak"]));

    // --- Ozellik degerlerini guncelle ---
    let (st, _) = ali.send(&app.app, "PATCH", &format!("/api/workspaces/{wid}/work-items/{iid}/attributes"),
        Some(json!({ "attributes": [ { "definition_id": metraj_id, "value": 4.2 } ] }))).await;
    assert_eq!(st, StatusCode::OK);
    let (_, det2) = ali.send(&app.app, "GET", &format!("/api/workspaces/{wid}/work-items/{iid}"), None).await;
    let m2 = det2["attributes"].as_array().unwrap().iter().find(|a| a["name"] == "Metraj").unwrap();
    assert_eq!(m2["value"], json!(4.2));

    // --- Filtreleme ---
    // metraj filtresi (sayisal)
    let (st, rows) = ali.send(&app.app, "GET",
        &format!("/api/workspaces/{wid}/work-items?q_metraj=4.2"), None).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(rows.as_array().unwrap().len(), 1);

    let (_, rows) = ali.send(&app.app, "GET",
        &format!("/api/workspaces/{wid}/work-items?q_metraj=999"), None).await;
    assert_eq!(rows.as_array().unwrap().len(), 0);

    // select filtresi
    let (_, rows) = ali.send(&app.app, "GET",
        &format!("/api/workspaces/{wid}/work-items?q_malzeme=Granit"), None).await;
    assert_eq!(rows.as_array().unwrap().len(), 1);
    let (_, rows) = ali.send(&app.app, "GET",
        &format!("/api/workspaces/{wid}/work-items?q_malzeme=Laminat"), None).await;
    assert_eq!(rows.as_array().unwrap().len(), 0);

    // multiselect icerir filtresi
    let (_, rows) = ali.send(&app.app, "GET",
        &format!("/api/workspaces/{wid}/work-items?q_etiketler=Acil"), None).await;
    assert_eq!(rows.as_array().unwrap().len(), 1);

    // isim arama
    let (_, rows) = ali.send(&app.app, "GET",
        &format!("/api/workspaces/{wid}/work-items?search=Tezgah"), None).await;
    assert_eq!(rows.as_array().unwrap().len(), 1);

    // --- Tip degisim korumasi (yol haritasi 18) ---
    let (st, _) = ali.send(&app.app, "PATCH",
        &format!("/api/workspaces/{wid}/work-types/{tid}/attributes/{metraj_id}"),
        Some(json!({ "data_type": "date" }))).await;
    assert_eq!(st, StatusCode::CONFLICT, "veri varken tip degismemeli");

    // veri olmayan tanimin tipi degisebilir
    let (st, _) = ali.send(&app.app, "POST",
        &format!("/api/workspaces/{wid}/work-types/{tid}/attributes"),
        Some(json!({ "name": "Notlar", "data_type": "text" }))).await;
    assert_eq!(st, StatusCode::CREATED);

    // --- Seri olusturma ---
    // Metraj zorunlu + default yok -> seri olusturulamaz
    let (st, _) = ali.send(&app.app, "POST", &format!("/api/workspaces/{wid}/work-items/bulk"),
        Some(json!({
            "section_id": blok_id,
            "work_type_id": tid,
            "template": "Tezgah {nn}",
            "start": 2, "end": 11
        }))).await;
    assert_eq!(st, StatusCode::BAD_REQUEST, "zorunlu + defaultsuz seri engellenmeli");

    // default verildikten sonra olusur
    let (st, _) = ali.send(&app.app, "PATCH",
        &format!("/api/workspaces/{wid}/work-types/{tid}/attributes/{metraj_id}"),
        Some(json!({ "default_value": "0" }))).await;
    assert_eq!(st, StatusCode::OK);

    let (st, res) = ali.send(&app.app, "POST", &format!("/api/workspaces/{wid}/work-items/bulk"),
        Some(json!({
            "section_id": blok_id,
            "work_type_id": tid,
            "template": "Tezgah {nn}",
            "start": 2, "end": 11
        }))).await;
    assert_eq!(st, StatusCode::OK, "default varken seri olusmali: {res:?}");
    assert_eq!(res["created"], 10);

    // toplam: 1 (tek) + 10 (seri) = 11
    let (_, rows) = ali.send(&app.app, "GET", &format!("/api/workspaces/{wid}/work-items"), None).await;
    assert_eq!(rows.as_array().unwrap().len(), 11);
}

#[tokio::test]
async fn bulk_with_required_no_default_fails() {
    let (app, mut ali, wid, blok_id, _) = bootstrap().await;

    let (_, wt) = ali.send(&app.app, "POST", &format!("/api/workspaces/{wid}/work-types"),
        Some(json!({ "name": "Boya" }))).await;
    let tid = wt["id"].as_str().unwrap().to_string();

    let (_, attr) = ali.send(&app.app, "POST",
        &format!("/api/workspaces/{wid}/work-types/{tid}/attributes"),
        Some(json!({ "name": "Renk", "data_type": "text", "is_required": true }))).await;
    let _ = attr;

    // zorunlu + defaultsuz -> seri olusturulamaz
    let (st, _) = ali.send(&app.app, "POST", &format!("/api/workspaces/{wid}/work-items/bulk"),
        Some(json!({ "section_id": blok_id, "work_type_id": tid, "template": "Duvar {n}", "start": 1, "end": 5 }))).await;
    assert_eq!(st, StatusCode::BAD_REQUEST);

    // default eklenince olusur
    let (_, defs) = ali.send(&app.app, "GET",
        &format!("/api/workspaces/{wid}/work-types/{tid}/attributes"), None).await;
    let renk_id = defs.as_array().unwrap()[0]["id"].as_str().unwrap().to_string();
    let (st, _) = ali.send(&app.app, "PATCH",
        &format!("/api/workspaces/{wid}/work-types/{tid}/attributes/{renk_id}"),
        Some(json!({ "default_value": "Beyaz" }))).await;
    assert_eq!(st, StatusCode::OK);

    let (st, res) = ali.send(&app.app, "POST", &format!("/api/workspaces/{wid}/work-items/bulk"),
        Some(json!({ "section_id": blok_id, "work_type_id": tid, "template": "Duvar {n}", "start": 1, "end": 5 }))).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(res["created"], 5);

    // default deger uygulanmis mi
    let (_, rows) = ali.send(&app.app, "GET",
        &format!("/api/workspaces/{wid}/work-items?q_renk=Beyaz"), None).await;
    assert_eq!(rows.as_array().unwrap().len(), 5);
}

#[tokio::test]
async fn work_item_scope_and_archive() {
    let (app, mut ali, wid, blok_id, _) = bootstrap().await;

    // veli Viewer olarak eklensin
    let mut veli = Client::new();
    veli.send(&app.app, "POST", "/api/auth/register",
        Some(json!({ "email": "veli@t.com", "password": "parola123", "name": "Veli" }))).await;
    let (_, roles) = ali.send(&app.app, "GET", &format!("/api/workspaces/{wid}/roles"), None).await;
    let viewer_id = roles.as_array().unwrap().iter()
        .find(|r| r["name"] == "Viewer").map(|r| r["id"].as_str().unwrap().to_string()).unwrap();
    let (st, _) = ali.send(&app.app, "POST", &format!("/api/workspaces/{wid}/members"),
        Some(json!({ "email": "veli@t.com", "role_id": viewer_id }))).await;
    assert_eq!(st, StatusCode::CREATED);

    // ali is kalemi olusturur
    let (_, wt) = ali.send(&app.app, "POST", &format!("/api/workspaces/{wid}/work-types"),
        Some(json!({ "name": "Montaj" }))).await;
    let (st, item) = ali.send(&app.app, "POST", &format!("/api/workspaces/{wid}/work-items"),
        Some(json!({ "section_id": blok_id, "work_type_id": wt["id"], "name": "Ana Montaj" }))).await;
    assert_eq!(st, StatusCode::CREATED);
    let iid = item["id"].as_str().unwrap().to_string();

    // Viewer is kalemi olusturamaz
    let (st, _) = veli.send(&app.app, "POST", &format!("/api/workspaces/{wid}/work-items"),
        Some(json!({ "section_id": blok_id, "name": "Izinsiz" }))).await;
    assert_eq!(st, StatusCode::FORBIDDEN);

    // goruntuleyebilir
    let (st, _) = veli.send(&app.app, "GET", &format!("/api/workspaces/{wid}/work-items/{iid}"), None).await;
    assert_eq!(st, StatusCode::OK);

    // kapsam disindan erisim: veli'yi baska bir subtree ile sinirla (baska workspace degil, ayni ws'de baska kok)
    let (_, blok_b) = ali.send(&app.app, "POST", &format!("/api/workspaces/{wid}/sections"),
        Some(json!({ "name": "Blok B" }))).await;
    let (_, members) = ali.send(&app.app, "GET", &format!("/api/workspaces/{wid}/members"), None).await;
    let veli_mid = members.as_array().unwrap().iter()
        .find(|m| m["email"] == "veli@t.com").map(|m| m["id"].as_str().unwrap().to_string()).unwrap();
    let (st, _) = ali.send(&app.app, "PUT", &format!("/api/workspaces/{wid}/members/{veli_mid}/scopes"),
        Some(json!({ "section_ids": [blok_b["id"]] }))).await;
    assert_eq!(st, StatusCode::OK);

    // artik eski item gorunmez
    let (st, _) = veli.send(&app.app, "GET", &format!("/api/workspaces/{wid}/work-items/{iid}"), None).await;
    assert_eq!(st, StatusCode::FORBIDDEN);
    let (_, rows) = veli.send(&app.app, "GET", &format!("/api/workspaces/{wid}/work-items"), None).await;
    assert_eq!(rows.as_array().unwrap().len(), 0);

    // ali arsivler, liste bosalir
    let (st, _) = ali.send(&app.app, "POST", &format!("/api/workspaces/{wid}/work-items/{iid}/archive" ), None).await;
    assert_eq!(st, StatusCode::OK);
    let (_, rows) = ali.send(&app.app, "GET", &format!("/api/workspaces/{wid}/work-items"), None).await;
    assert_eq!(rows.as_array().unwrap().len(), 0);
    let (_, rows) = ali.send(&app.app, "GET", &format!("/api/workspaces/{wid}/work-items?include_archived=true"), None).await;
    assert_eq!(rows.as_array().unwrap().len(), 1);
}
