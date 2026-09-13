//! Uctan uca akis testleri: kayit -> workspace -> uye/rol -> kapsam -> bolum agaci.
//! Her test ayri in-memory SQLite kullanir.

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

    fn req(&self, method: &str, uri: &str, body: Option<Value>) -> Request<Body> {
        let mut builder = Request::builder().method(method).uri(uri);
        if let Some(c) = &self.cookie {
            builder = builder.header(header::COOKIE, c);
        }
        match body {
            Some(v) => builder
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(v.to_string()))
                .unwrap(),
            None => builder.body(Body::empty()).unwrap(),
        }
    }

    /// Istek gonderir; set-cookie varsa saklar.
    async fn send(&mut self, app: &Router, method: &str, uri: &str, body: Option<Value>) -> (StatusCode, Value) {
        let res = app.clone().oneshot(self.req(method, uri, body)).await.unwrap();
        if let Some(sc) = res.headers().get(header::SET_COOKIE) {
            let sc = sc.to_str().unwrap().to_string();
            // "imtk_session=xyz; Path=/; ..." -> sadece token kismi
            let pair = sc.split(';').next().unwrap().to_string();
            if pair.starts_with("imtk_session=") && pair != "imtk_session=" {
                self.cookie = Some(pair);
            } else if pair.contains('=') && !pair.ends_with('=') {
                // silme cookie'si - bosver
            } else {
                self.cookie = None;
            }
        }
        let status = res.status();
        let bytes = axum::body::to_bytes(res.into_body(), 1024 * 1024)
            .await
            .unwrap();
        let body: Value = if bytes.is_empty() {
            Value::Null
        } else {
            serde_json::from_slice(&bytes).unwrap_or(Value::Null)
        };
        (status, body)
    }
}

#[tokio::test]
async fn full_flow_auth_workspace_permissions_sections() {
    let app = setup().await;

    // --- 1. Kayit ---
    let mut ali = Client::new();
    let (st, _) = ali
        .send(
            &app.app,
            "POST",
            "/api/auth/register",
            Some(json!({ "email": "ali@test.com", "password": "parola123", "name": "Ali" })),
        )
        .await;
    assert_eq!(st, StatusCode::OK, "kayit basarili olmali");

    let mut veli = Client::new();
    let (st, _) = veli
        .send(
            &app.app,
            "POST",
            "/api/auth/register",
            Some(json!({ "email": "veli@test.com", "password": "parola123", "name": "Veli" })),
        )
        .await;
    assert_eq!(st, StatusCode::OK);

    // email auth'suz erisemez
    let mut anon = Client::new();
    let (st, _) = anon.send(&app.app, "GET", "/api/auth/me", None).await;
    assert_eq!(st, StatusCode::UNAUTHORIZED);

    // yanlis sifre
    let (st, _) = veli
        .send(
            &app.app,
            "POST",
            "/api/auth/login",
            Some(json!({ "email": "veli@test.com", "password": "yanlis" })),
        )
        .await;
    assert_eq!(st, StatusCode::UNAUTHORIZED);

    // --- 2. Workspace olusturma ---
    let (st, ws) = ali
        .send(
            &app.app,
            "POST",
            "/api/workspaces",
            Some(json!({ "name": "Atolye Merkez" })),
        )
        .await;
    assert_eq!(st, StatusCode::CREATED);
    let wid = ws["id"].as_str().unwrap().to_string();
    assert_eq!(ws["slug"], "atolye-merkez");

    // --- 3. Sistem rolleri ---
    let (st, roles) = ali
        .send(&app.app, "GET", &format!("/api/workspaces/{wid}/roles"), None)
        .await;
    assert_eq!(st, StatusCode::OK);
    let roles = roles.as_array().unwrap();
    assert_eq!(roles.len(), 5, "5 varsayilan sistem rolu olusmali");
    let viewer_id = roles
        .iter()
        .find(|r| r["name"] == "Viewer")
        .map(|r| r["id"].as_str().unwrap().to_string())
        .unwrap();
    let supervisor_id = roles
        .iter()
        .find(|r| r["name"] == "Supervisor")
        .map(|r| r["id"].as_str().unwrap().to_string())
        .unwrap();

    // --- 4. Uye olmayan erisemez (varlik bilgisi de sizmaz) ---
    let (st, _) = veli
        .send(&app.app, "GET", &format!("/api/workspaces/{wid}"), None)
        .await;
    assert_eq!(st, StatusCode::NOT_FOUND);

    // --- 5. Uye ekleme (Viewer) ---
    let (st, member) = ali
        .send(
            &app.app,
            "POST",
            &format!("/api/workspaces/{wid}/members"),
            Some(json!({ "email": "veli@test.com", "role_id": viewer_id })),
        )
        .await;
    assert_eq!(st, StatusCode::CREATED, "uye eklenebilmeli: {member:?}");
    let mid = member["id"].as_str().unwrap().to_string();

    // ayni uye tekrar eklenemez
    let (st, _) = ali
        .send(
            &app.app,
            "POST",
            &format!("/api/workspaces/{wid}/members"),
            Some(json!({ "email": "veli@test.com", "role_id": viewer_id })),
        )
        .await;
    assert_eq!(st, StatusCode::CONFLICT);

    // --- 6. Viewer bolum olusturamaz ---
    let (st, _) = veli
        .send(
            &app.app,
            "POST",
            &format!("/api/workspaces/{wid}/sections"),
            Some(json!({ "name": "Blok A" })),
        )
        .await;
    assert_eq!(st, StatusCode::FORBIDDEN, "Viewer icin 403 beklenir");

    // --- 7. Rol yukseltme: Supervisor ---
    let (st, _) = ali
        .send(
            &app.app,
            "PATCH",
            &format!("/api/workspaces/{wid}/members/{mid}"),
            Some(json!({ "role_id": supervisor_id })),
        )
        .await;
    assert_eq!(st, StatusCode::OK);

    // Supervisor artik bolum olusturabilir (scope yok = tam erisim)
    let (st, blok_a) = veli
        .send(
            &app.app,
            "POST",
            &format!("/api/workspaces/{wid}/sections"),
            Some(json!({ "name": "Blok A" })),
        )
        .await;
    assert_eq!(st, StatusCode::CREATED, "Supervisor bolum olusturabilmeli");
    let blok_a_id = blok_a["id"].as_str().unwrap().to_string();

    // --- 8. Seri bolum olusturma ---
    let (st, res) = ali
        .send(
            &app.app,
            "POST",
            &format!("/api/workspaces/{wid}/sections/serial"),
            Some(json!({ "parent_id": blok_a_id, "template": "{parent} Daire {nn}", "start": 1, "end": 5 })),
        )
        .await;
    assert_eq!(st, StatusCode::OK, "seri olusturma: {res:?}");
    assert_eq!(res["created"], 5);

    let (st, sections) = ali
        .send(&app.app, "GET", &format!("/api/workspaces/{wid}/sections"), None)
        .await;
    assert_eq!(st, StatusCode::OK);
    let sections = sections.as_array().unwrap();
    assert_eq!(sections.len(), 6, "1 blok + 5 daire");
    let daire1 = sections
        .iter()
        .find(|s| s["name"] == "Blok A Daire 01")
        .map(|s| s["id"].as_str().unwrap().to_string())
        .expect("sablon isimlendirme calismali");

    // --- 9. Kapsam (scope) kisitlamasi ---
    // Ali, Veli'nin erisimini sadece Blok A subtree'si ile sinirlandirir.
    // Beklenti: Veli yine gorur (Blok A kapsaminda), ama kok seviyede yeni bolum acamaz.
    let (st, _) = ali
        .send(
            &app.app,
            "PUT",
            &format!("/api/workspaces/{wid}/members/{mid}/scopes"),
            Some(json!({ "section_ids": [blok_a_id] })),
        )
        .await;
    assert_eq!(st, StatusCode::OK);

    let (st, visible) = veli
        .send(&app.app, "GET", &format!("/api/workspaces/{wid}/sections"), None)
        .await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(
        visible.as_array().unwrap().len(),
        6,
        "Blok A + 5 daire gorunmeli (tumu kapsamda)"
    );

    // Kok seviyede bolum olusturamaz artik
    let (st, _) = veli
        .send(
            &app.app,
            "POST",
            &format!("/api/workspaces/{wid}/sections"),
            Some(json!({ "name": "Blok B" })),
        )
        .await;
    assert_eq!(st, StatusCode::FORBIDDEN, "kapsam disi kok olusturma engellenmeli");

    // Kapsam icinde bolum olusturabilir
    let (st, _) = veli
        .send(
            &app.app,
            "POST",
            &format!("/api/workspaces/{wid}/sections"),
            Some(json!({ "parent_id": daire1, "name": "Mutfak" })),
        )
        .await;
    assert_eq!(st, StatusCode::CREATED);

    // Scope kaldirilirsa tam erisim doner
    let (st, _) = ali
        .send(
            &app.app,
            "PUT",
            &format!("/api/workspaces/{wid}/members/{mid}/scopes"),
            Some(json!({ "section_ids": [] })),
        )
        .await;
    assert_eq!(st, StatusCode::OK);
    let (st, _) = veli
        .send(
            &app.app,
            "POST",
            &format!("/api/workspaces/{wid}/sections"),
            Some(json!({ "name": "Blok B" })),
        )
        .await;
    assert_eq!(st, StatusCode::CREATED);

    // --- 10. Klonlama ---
    let (st, klon) = ali
        .send(
            &app.app,
            "POST",
            &format!("/api/workspaces/{wid}/sections/{blok_a_id}/clone"),
            Some(json!({})),
        )
        .await;
    assert_eq!(st, StatusCode::CREATED);
    assert_eq!(klon["name"], "Blok A (Kopya)");

    let (_st, sections) = ali
        .send(&app.app, "GET", &format!("/api/workspaces/{wid}/sections"), None)
        .await;
    let count = sections.as_array().unwrap().len();
    // Blok A(1) + 5 daire + Mutfak(1) + Blok B(1) + klon(1 + 5 daire + 1 mutfak = 7) = 15
    assert_eq!(count, 15, "klon alt agaci ile birlikte gelmeli: {count}");

    // --- 11. Tasma + cycle korumasi ---
    let (_st, sections) = ali
        .send(&app.app, "GET", &format!("/api/workspaces/{wid}/sections"), None)
        .await;
    let secs = sections.as_array().unwrap();
    let mutfak_id = secs
        .iter()
        .find(|s| s["name"] == "Mutfak")
        .map(|s| s["id"].as_str().unwrap().to_string())
        .unwrap();
    let blok_b_id = secs
        .iter()
        .find(|s| s["name"] == "Blok B")
        .map(|s| s["id"].as_str().unwrap().to_string())
        .unwrap();

    // Cycle: Daire 01'i, kendi çocuğu Mutfak'ın altına taşımaya çalışmak -> 400
    // (taşıma öncesi Mutfak hâlâ Daire 01'in çocuğu)
    let (st, _) = ali
        .send(
            &app.app,
            "POST",
            &format!("/api/workspaces/{wid}/sections/{daire1}/move"),
            Some(json!({ "parent_id": mutfak_id })),
        )
        .await;
    assert_eq!(st, StatusCode::BAD_REQUEST, "cycle engellenmeli");

    // Mesru tasma: Mutfak (derin) -> Blok B'nin altina
    let (st, _) = ali
        .send(
            &app.app,
            "POST",
            &format!("/api/workspaces/{wid}/sections/{mutfak_id}/move"),
            Some(json!({ "parent_id": blok_b_id })),
        )
        .await;
    assert_eq!(st, StatusCode::OK);

    // --- 12. Arsivleme (subtree) ---
    let (st, res) = ali
        .send(
            &app.app,
            "POST",
            &format!("/api/workspaces/{wid}/sections/{blok_a_id}/archive"),
            None,
        )
        .await;
    assert_eq!(st, StatusCode::OK);
    assert!(res["archived"].as_i64().unwrap() >= 6, "tum alt agac arsivlenmeli");

    // --- 13. Takim ---
    let (st, team) = ali
        .send(
            &app.app,
            "POST",
            &format!("/api/workspaces/{wid}/teams"),
            Some(json!({ "name": "Montaj Ekibi", "description": "Ana hat" })),
        )
        .await;
    assert_eq!(st, StatusCode::CREATED);
    let tid = team["id"].as_str().unwrap().to_string();

    let veli_user: Value = veli.send(&app.app, "GET", "/api/auth/me", None).await.1;
    let veli_uid = veli_user["id"].as_str().unwrap().to_string();

    let (st, _) = ali
        .send(
            &app.app,
            "POST",
            &format!("/api/workspaces/{wid}/teams/{tid}/members"),
            Some(json!({ "user_id": veli_uid })),
        )
        .await;
    assert_eq!(st, StatusCode::OK);

    let (st, teams) = ali
        .send(&app.app, "GET", &format!("/api/workspaces/{wid}/teams"), None)
        .await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(teams[0]["member_count"], 1);

    // --- 14. Logout ---
    let (st, _) = veli.send(&app.app, "POST", "/api/auth/logout", None).await;
    assert_eq!(st, StatusCode::OK);
    let (st, _) = veli.send(&app.app, "GET", "/api/auth/me", None).await;
    assert_eq!(st, StatusCode::UNAUTHORIZED, "cikis sonrasi oturum gecersiz");
}

#[tokio::test]
async fn workspace_isolation_and_validation() {
    let app = setup().await;

    let mut a = Client::new();
    a.send(&app.app, "POST", "/api/auth/register",
        Some(json!({ "email": "a@t.com", "password": "parola123", "name": "A" }))).await;

    // ayni email tekrar
    let (st, _) = a.send(&app.app, "POST", "/api/auth/register",
        Some(json!({ "email": "a@t.com", "password": "parola123", "name": "A" }))).await;
    assert_eq!(st, StatusCode::CONFLICT);

    // kisa sifre
    let (st, _) = a.send(&app.app, "POST", "/api/auth/register",
        Some(json!({ "email": "b@t.com", "password": "123", "name": "B" }))).await;
    assert_eq!(st, StatusCode::BAD_REQUEST);

    // iki workspace ayni isim - slug farkli olmali
    let (_, w1) = a.send(&app.app, "POST", "/api/workspaces",
        Some(json!({ "name": "Depo" }))).await;
    let (_, w2) = a.send(&app.app, "POST", "/api/workspaces",
        Some(json!({ "name": "Depo" }))).await;
    assert_ne!(w1["slug"], w2["slug"], "slug cakismasi cozulmeli");

    // workspace listesi
    let (st, list) = a.send(&app.app, "GET", "/api/workspaces", None).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(list.as_array().unwrap().len(), 2);

    // kayitli olmayan email ile uye ekleme
    let wid = w1["id"].as_str().unwrap();
    let (st, _) = a.send(&app.app, "POST", &format!("/api/workspaces/{wid}/members"),
        Some(json!({ "email": "yok@t.com", "role_id": "x" }))).await;
    assert_eq!(st, StatusCode::BAD_REQUEST);

    // Owner rolu ile uye ekleme denemesi reddedilmeli
    let (_, roles) = a.send(&app.app, "GET", &format!("/api/workspaces/{wid}/roles"), None).await;
    let owner_id = roles.as_array().unwrap().iter()
        .find(|r| r["name"] == "Owner").map(|r| r["id"].as_str().unwrap().to_string()).unwrap();
    let (st, _) = a.send(&app.app, "POST", &format!("/api/workspaces/{wid}/members"),
        Some(json!({ "email": "a@t.com", "role_id": owner_id }))).await;
    assert_eq!(st, StatusCode::BAD_REQUEST, "Owner rolu atanamaz");
}
