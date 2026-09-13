//! E-posta davet sistemi: kayitli olmayan kullaniciyi token'li link ile workspace'e cagirma.
//! Kabul: hesap olusturma + uyelik + oturum tek akista.

use axum::extract::{Path, State};
use axum::{routing, Json, Router};
use serde::{Deserialize, Serialize};

use crate::auth::{AuthUser, CurrentUser, SessionService, SESSION_COOKIE};
use crate::error::{AppError, AppResult};
use crate::state::AppState;
use crate::util;
use crate::ws_ctx::WsCtx;

const INVITE_DAYS: i64 = 7;

#[derive(Serialize, sqlx::FromRow)]
pub struct InviteRow {
    pub id: String,
    pub email: String,
    pub role_name: String,
    pub status: String,
    pub expires_at: String,
    pub created_at: String,
    pub invited_by_name: String,
}

#[derive(Deserialize)]
pub struct CreateInviteReq {
    email: String,
    role_id: String,
}

#[derive(Deserialize)]
pub struct AcceptInviteReq {
    name: String,
    password: String,
}

fn valid_email(e: &str) -> bool {
    e.contains('@') && e.len() >= 5 && !e.starts_with('@') && !e.ends_with('@')
}

/// Davet olustur: token uret + davet e-postasini outbox'a kuyrukla.
pub async fn create_invite(
    State(state): State<AppState>,
    user: AuthUser,
    Path(wid): Path<String>,
    Json(req): Json<CreateInviteReq>,
) -> AppResult<(axum::http::StatusCode, Json<serde_json::Value>)> {
    let ctx = WsCtx::load(&state.db, &user, &wid).await?;
    ctx.require("user.invite")?;

    let email = req.email.trim().to_lowercase();
    if !valid_email(&email) {
        return Err(AppError::BadRequest("Gecerli bir e-posta girin".into()));
    }

    // Rol dogrulama (Owner harici)
    let role: Option<(String,)> = sqlx::query_as(
        "SELECT id FROM roles WHERE id = ?1 AND workspace_id = ?2 AND name != 'Owner'",
    )
    .bind(&req.role_id)
    .bind(&wid)
    .fetch_optional(&state.db)
    .await?;
    if role.is_none() {
        return Err(AppError::BadRequest("Gecersiz rol".into()));
    }

    // Zaten uye mi?
    let already: Option<(String,)> = sqlx::query_as(
        "SELECT id FROM workspace_members WHERE workspace_id = ?1 AND user_id IN (SELECT id FROM users WHERE email = ?2) AND archived_at IS NULL",
    )
    .bind(&wid)
    .bind(&email)
    .fetch_optional(&state.db)
    .await?;
    if already.is_some() {
        return Err(AppError::Conflict("Bu e-posta zaten uye".into()));
    }

    // Bekleyen davet var mi?
    let pending: Option<(String,)> = sqlx::query_as(
        "SELECT id FROM workspace_invites WHERE workspace_id = ?1 AND email = ?2 AND status = 'pending'",
    )
    .bind(&wid)
    .bind(&email)
    .fetch_optional(&state.db)
    .await?;
    if pending.is_some() {
        return Err(AppError::Conflict("Bu e-postaya zaten bekleyen davet var".into()));
    }

    // Workspace + davet eden bilgisi
    let ws: (String,) = sqlx::query_as("SELECT name FROM workspaces WHERE id = ?1")
        .bind(&wid)
        .fetch_one(&state.db)
        .await?;

    let token = util::random_token();
    let id = util::new_id();
    let now = util::now();
    let expires = util::now_plus_days(INVITE_DAYS);

    sqlx::query(
        "INSERT INTO workspace_invites (id, workspace_id, email, role_id, token, status, expires_at, invited_by, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, 'pending', ?6, ?7, ?8)",
    )
    .bind(&id)
    .bind(&wid)
    .bind(&email)
    .bind(&req.role_id)
    .bind(&token)
    .bind(&expires)
    .bind(&user.0.id)
    .bind(&now)
    .execute(&state.db)
    .await?;

    // Davet e-postasini kuyruga yaz (worker SMTP'den gonderir; yoksa simule)
    let link = format!("{}/davet/{}", state.config.app_url, token);
    let subject = format!("{} sizi \"{}\" workspace'ine davet etti", user.0.name, ws.0);
    let body = format!(
        "Merhaba,\n\n{} sizi \"{}\" workspace'ine davet etti.\n\nDaveti kabul etmek için bağlantıyı açın:\n{}\n\nBağlantı {} tarihine kadar geçerlidir.\n\n— Atölye Takip",
        user.0.name, ws.0, link, expires
    );
    sqlx::query(
        "INSERT INTO email_outbox (id, workspace_id, to_email, subject, body, status, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, 'pending', ?6)",
    )
    .bind(util::new_id())
    .bind(&wid)
    .bind(&email)
    .bind(&subject)
    .bind(&body)
    .bind(&now)
    .execute(&state.db)
    .await?;

    Ok((
        axum::http::StatusCode::CREATED,
        Json(serde_json::json!({ "id": id, "token": token, "expires_at": expires })),
    ))
}

/// Davet listesi (yonetici).
pub async fn list_invites(
    State(state): State<AppState>,
    user: AuthUser,
    Path(wid): Path<String>,
) -> AppResult<Json<Vec<InviteRow>>> {
    let ctx = WsCtx::load(&state.db, &user, &wid).await?;
    ctx.require("user.invite")?;

    let rows = sqlx::query_as::<_, InviteRow>(
        "SELECT i.id, i.email, r.name AS role_name, i.status, i.expires_at, i.created_at,
                (SELECT u.name FROM users u WHERE u.id = i.invited_by) AS invited_by_name
         FROM workspace_invites i
         JOIN roles r ON r.id = i.role_id
         WHERE i.workspace_id = ?1
         ORDER BY i.created_at DESC LIMIT 100",
    )
    .bind(&wid)
    .fetch_all(&state.db)
    .await?;
    Ok(Json(rows))
}

/// Davet iptali (pending -> cancelled).
pub async fn cancel_invite(
    State(state): State<AppState>,
    user: AuthUser,
    Path((wid, iid)): Path<(String, String)>,
) -> AppResult<Json<serde_json::Value>> {
    let ctx = WsCtx::load(&state.db, &user, &wid).await?;
    ctx.require("user.invite")?;
    let res = sqlx::query(
        "UPDATE workspace_invites SET status = 'cancelled' WHERE id = ?1 AND workspace_id = ?2 AND status = 'pending'",
    )
    .bind(&iid)
    .bind(&wid)
    .execute(&state.db)
    .await?;
    if res.rows_affected() == 0 {
        return Err(AppError::NotFound("Davet bulunamadi ya da zaten sonlanmis".into()));
    }
    Ok(Json(serde_json::json!({ "ok": true })))
}

// ---------------------------------------------------------------------------
// Public: davet bilgisi + kabul
// ---------------------------------------------------------------------------

#[derive(Serialize)]
pub struct InviteInfo {
    pub workspace_name: String,
    pub email: String,
    pub inviter_name: String,
    pub role_name: String,
    pub expires_at: String,
    /// hesabi varsa true (sadece giris yeterli); yoksa kayit formu gosterilir
    pub existing_account: bool,
}

async fn load_valid_invite(
    db: &sqlx::SqlitePool,
    token: &str,
) -> AppResult<(String, String, String, String, String)> {
    // (id, email, role_id, workspace_id, expires_at)
    let row: Option<(String, String, String, String, String, String)> = sqlx::query_as(
        "SELECT id, email, role_id, workspace_id, status, expires_at FROM workspace_invites WHERE token = ?1",
    )
    .bind(token)
    .fetch_optional(db)
    .await?;
    let (id, email, role_id, wid, status, expires) = row
        .ok_or_else(|| AppError::NotFound("Davet bulunamadi".into()))?;
    if status == "cancelled" {
        return Err(AppError::BadRequest("Bu davet iptal edilmis".into()));
    }
    if status == "accepted" {
        return Err(AppError::BadRequest("Bu davet zaten kullanilmis".into()));
    }
    if expires < util::now() {
        return Err(AppError::BadRequest("Bu davetin suresi dolmus".into()));
    }
    Ok((id, email, role_id, wid, expires))
}

/// Davet bilgisi (kabul ekranini doldurur) — token ile public.
pub async fn invite_info(
    State(state): State<AppState>,
    Path(token): Path<String>,
) -> AppResult<Json<InviteInfo>> {
    let (_, email, _, wid, expires) = load_valid_invite(&state.db, &token).await?;

    let ws: (String,) = sqlx::query_as("SELECT name FROM workspaces WHERE id = ?1")
        .bind(&wid)
        .fetch_one(&state.db)
        .await?;
    let extra: (String, String) = sqlx::query_as(
        "SELECT (SELECT u.name FROM users u WHERE u.id = i.invited_by),
                (SELECT r.name FROM roles r WHERE r.id = i.role_id)
         FROM workspace_invites i WHERE i.token = ?1",
    )
    .bind(&token)
    .fetch_one(&state.db)
    .await?;
    let existing: Option<(String,)> =
        sqlx::query_as("SELECT id FROM users WHERE email = ?1")
            .bind(&email)
            .fetch_optional(&state.db)
            .await?;

    Ok(Json(InviteInfo {
        workspace_name: ws.0,
        email,
        inviter_name: extra.0,
        role_name: extra.1,
        expires_at: expires,
        existing_account: existing.is_some(),
    }))
}

/// Daveti kabul et: hesap yoksa olustur + workspace'e ekle + oturum ac.
pub async fn accept_invite(
    State(state): State<AppState>,
    jar: axum_extra::extract::CookieJar,
    Path(token): Path<String>,
    Json(req): Json<AcceptInviteReq>,
) -> AppResult<axum::response::Response> {
    use axum::response::IntoResponse;

    let (invite_id, email, role_id, wid, _) = load_valid_invite(&state.db, &token).await?;

    let name = req.name.trim().to_string();
    if name.is_empty() {
        return Err(AppError::BadRequest("Isim bos olamaz".into()));
    }

    // Hesap: varsa dogrula (sifre eslesmeli), yoksa olustur
    let user_id: String = match sqlx::query_as::<_, (String, String, i64)>(
        "SELECT id, password_hash, is_active FROM users WHERE email = ?1",
    )
    .bind(&email)
    .fetch_optional(&state.db)
    .await?
    {
        Some((id, hash, active)) => {
            if active != 1 {
                return Err(AppError::Forbidden("Hesap pasif durumda".into()));
            }
            use argon2::password_hash::PasswordVerifier;
            let parsed = argon2::PasswordHash::new(&hash).map_err(|_| AppError::Unauthorized)?;
            if argon2::Argon2::default()
                .verify_password(req.password.as_bytes(), &parsed)
                .is_err()
            {
                return Err(AppError::BadRequest(
                    "Bu e-posta ile kayitli hesap var; sifre eslesmiyor".into(),
                ));
            }
            id
        }
        None => {
            if req.password.len() < 8 {
                return Err(AppError::BadRequest("Sifre en az 8 karakter olmali".into()));
            }
            use argon2::password_hash::{PasswordHasher, SaltString};
            let salt = SaltString::generate(&mut argon2::password_hash::rand_core::OsRng);
            let hash = argon2::Argon2::default()
                .hash_password(req.password.as_bytes(), &salt)
                .map_err(|e| AppError::BadRequest(format!("Sifre islenemedi: {e}")))?
                .to_string();
            let id = util::new_id();
            let now = util::now();
            sqlx::query(
                "INSERT INTO users (id, email, password_hash, name, is_active, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, 1, ?5, ?5)",
            )
            .bind(&id)
            .bind(&email)
            .bind(&hash)
            .bind(&name)
            .bind(&now)
            .execute(&state.db)
            .await?;
            id
        }
    };

    // Uyelik (yoksa ekle)
    let already: Option<(String,)> = sqlx::query_as(
        "SELECT id FROM workspace_members WHERE workspace_id = ?1 AND user_id = ?2",
    )
    .bind(&wid)
    .bind(&user_id)
    .fetch_optional(&state.db)
    .await?;
    if already.is_none() {
        let now = util::now();
        sqlx::query(
            "INSERT INTO workspace_members (id, workspace_id, user_id, role_id, status, joined_at, created_at)
             VALUES (?1, ?2, ?3, ?4, 'active', ?5, ?5)",
        )
        .bind(util::new_id())
        .bind(&wid)
        .bind(&user_id)
        .bind(&role_id)
        .bind(&now)
        .execute(&state.db)
        .await?;
        crate::audit::audit(&state.db, &wid, Some(&user_id), "member.added", "workspace_member", &user_id,
            serde_json::json!({ "via": "invite", "email": email })).await;
    }

    // Daveti kapat
    sqlx::query(
        "UPDATE workspace_invites SET status = 'accepted', accepted_at = ?1 WHERE id = ?2",
    )
    .bind(util::now())
    .bind(&invite_id)
    .execute(&state.db)
    .await?;

    // Oturum ac
    let token_val = SessionService::create(&state.db, &user_id, None).await?;
    let cookie = axum_extra::extract::cookie::Cookie::build((SESSION_COOKIE, token_val))
        .path("/")
        .http_only(true)
        .same_site(axum_extra::extract::cookie::SameSite::Lax)
        .max_age(time::Duration::days(30))
        .secure(state.config.cookie_secure)
        .build();
    let jar = jar.add(cookie);

    Ok((
        jar,
        Json(serde_json::json!({ "ok": true, "workspace_id": wid, "user": CurrentUser { id: user_id, email, name } })),
    )
        .into_response())
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/workspaces/{wid}/invites",
            routing::get(list_invites).post(create_invite),
        )
        .route(
            "/workspaces/{wid}/invites/{iid}",
            routing::delete(cancel_invite),
        )
        .route("/invites/{token}", routing::get(invite_info))
        .route("/invites/{token}/accept", routing::post(accept_invite))
}
