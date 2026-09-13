use axum::extract::{FromRequestParts, State};
use axum::http::request::Parts;
use axum_extra::extract::cookie::{Cookie, SameSite};
use axum_extra::extract::CookieJar;
use axum::{routing, Router};
use serde::Deserialize;
use sqlx::SqlitePool;

use crate::error::{AppError, AppResult};
use crate::state::AppState;
use crate::util;

pub const SESSION_COOKIE: &str = "imtk_session";
const SESSION_DAYS: i64 = 30;

// ---------------------------------------------------------------------------
// Session yonetimi
// ---------------------------------------------------------------------------

pub struct SessionService;

impl SessionService {
    pub async fn create(
        db: &SqlitePool,
        user_id: &str,
        user_agent: Option<&str>,
    ) -> AppResult<String> {
        let token = util::random_token();
        let now = util::now();
        let expires = util::now_plus_days(SESSION_DAYS);
        sqlx::query(
            "INSERT INTO sessions (id, user_id, token_hash, user_agent, expires_at, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        )
        .bind(util::new_id())
        .bind(user_id)
        .bind(util::hash_token(&token))
        .bind(user_agent)
        .bind(&expires)
        .bind(&now)
        .execute(db)
        .await?;
        Ok(token)
    }

    pub async fn resolve_user(db: &SqlitePool, token: &str) -> AppResult<CurrentUser> {
        let hash = util::hash_token(token);
        let user = sqlx::query_as::<_, CurrentUser>(
            "SELECT u.id, u.email, u.name
             FROM sessions s
             JOIN users u ON u.id = s.user_id
             WHERE s.token_hash = ?1
               AND s.expires_at > ?2
               AND u.is_active = 1",
        )
        .bind(&hash)
        .bind(util::now())
        .fetch_optional(db)
        .await?
        .ok_or(AppError::Unauthorized)?;
        Ok(user)
    }

    pub async fn delete(db: &SqlitePool, token: &str) -> AppResult<()> {
        sqlx::query("DELETE FROM sessions WHERE token_hash = ?1")
            .bind(util::hash_token(token))
            .execute(db)
            .await?;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Extractor: istegi yapan kullanici
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize)]
pub struct CurrentUser {
    pub id: String,
    pub email: String,
    pub name: String,
}

pub struct AuthUser(pub CurrentUser);

impl FromRequestParts<AppState> for AuthUser {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let jar = CookieJar::from_headers(&parts.headers);
        let token = jar
            .get(SESSION_COOKIE)
            .map(|c| c.value().to_string())
            .ok_or(AppError::Unauthorized)?;
        let user = SessionService::resolve_user(&state.db, &token).await?;
        Ok(AuthUser(user))
    }
}

// ---------------------------------------------------------------------------
// Handler'lar
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct RegisterReq {
    email: String,
    password: String,
    name: String,
}

#[derive(Deserialize)]
pub struct LoginReq {
    email: String,
    password: String,
}

fn validate_email(email: &str) -> AppResult<String> {
    let e = email.trim().to_lowercase();
    if !e.contains('@') || e.len() < 5 || e.starts_with('@') || e.ends_with('@') {
        return Err(AppError::BadRequest("Gecerli bir e-posta adresi girin".into()));
    }
    Ok(e)
}

fn set_session_cookie(jar: CookieJar, token: &str, secure: bool) -> CookieJar {
    let cookie = Cookie::build((SESSION_COOKIE, token.to_string()))
        .path("/")
        .http_only(true)
        .same_site(SameSite::Lax)
        .max_age(time::Duration::days(SESSION_DAYS))
        .secure(secure)
        .build();
    jar.add(cookie)
}

use axum::response::IntoResponse;

pub async fn register(
    State(state): State<AppState>,
    jar: CookieJar,
    axum::Json(req): axum::Json<RegisterReq>,
) -> AppResult<axum::response::Response> {
    let email = validate_email(&req.email)?;
    let name = req.name.trim().to_string();
    if name.is_empty() {
        return Err(AppError::BadRequest("Isim bos olamaz".into()));
    }
    if req.password.len() < 8 {
        return Err(AppError::BadRequest("Sifre en az 8 karakter olmali".into()));
    }

    let exists: Option<(String,)> =
        sqlx::query_as("SELECT id FROM users WHERE email = ?1")
            .bind(&email)
            .fetch_optional(&state.db)
            .await?;
    if exists.is_some() {
        return Err(AppError::Conflict("Bu e-posta ile kayitli bir hesap var".into()));
    }

    // Argon2id hash
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

    let token = SessionService::create(&state.db, &id, None).await?;
    let user = CurrentUser { id, email, name };
    let jar = set_session_cookie(jar, &token, state.config.cookie_secure);
    Ok((jar, axum::Json(user)).into_response())
}

pub async fn login(
    State(state): State<AppState>,
    jar: CookieJar,
    headers: axum::http::HeaderMap,
    axum::Json(req): axum::Json<LoginReq>,
) -> AppResult<axum::response::Response> {
    let email = req.email.trim().to_lowercase();
    let row: Option<(String, String, String, i64)> = sqlx::query_as(
        "SELECT id, password_hash, name, is_active FROM users WHERE email = ?1",
    )
    .bind(&email)
    .fetch_optional(&state.db)
    .await?;

    let (user_id, hash, name, active) = match row {
        Some(r) => r,
        None => return Err(AppError::Unauthorized),
    };
    if active != 1 {
        return Err(AppError::Forbidden("Hesap pasif durumda".into()));
    }

    use argon2::password_hash::PasswordVerifier;
    let parsed = argon2::PasswordHash::new(&hash).map_err(|_| AppError::Unauthorized)?;
    if argon2::Argon2::default()
        .verify_password(req.password.as_bytes(), &parsed)
        .is_err()
    {
        return Err(AppError::Unauthorized);
    }

    let ua = headers
        .get(axum::http::header::USER_AGENT)
        .and_then(|v| v.to_str().ok());
    let token = SessionService::create(&state.db, &user_id, ua).await?;
    let jar = set_session_cookie(jar, &token, state.config.cookie_secure);
    let user = CurrentUser {
        id: user_id,
        email,
        name,
    };
    Ok((jar, axum::Json(user)).into_response())
}

pub async fn logout(
    State(state): State<AppState>,
    jar: CookieJar,
) -> AppResult<axum::response::Response> {
    if let Some(cookie) = jar.get(SESSION_COOKIE) {
        let _ = SessionService::delete(&state.db, cookie.value()).await;
    }
    let mut removal = Cookie::new(SESSION_COOKIE, "");
    removal.set_path("/");
    removal.make_removal();
    let jar = jar.add(removal);
    Ok((jar, axum::Json(serde_json::json!({ "ok": true }))).into_response())
}

pub async fn me(user: AuthUser) -> AppResult<axum::Json<CurrentUser>> {
    Ok(axum::Json(user.0))
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/auth/register", routing::post(register))
        .route("/auth/login", routing::post(login))
        .route("/auth/logout", routing::post(logout))
        .route("/auth/me", routing::get(me))
}
