use std::env;

#[derive(Clone, Debug)]
pub struct Config {
    pub host: String,
    pub port: u16,
    pub db_url: String,
    /// Production: SvelteKit adapter-static build klasoru (bos ise statik servis kapali)
    pub static_dir: Option<String>,
    pub cookie_secure: bool,
    // SMTP (doluysa gercek gonderim; bos ise simulasyon)
    pub smtp_host: String,
    pub smtp_port: u16,
    pub smtp_user: String,
    pub smtp_pass: String,
    pub smtp_from: String,
    /// Davet linkleri icin base URL (orn. http://localhost:5173)
    pub app_url: String,
}

impl Config {
    pub fn smtp_configured(&self) -> bool {
        !self.smtp_host.is_empty() && !self.smtp_user.is_empty() && !self.smtp_pass.is_empty()
    }
}

impl Config {
    pub fn from_env() -> Self {
        let port = env::var("IMTK_PORT")
            .ok()
            .and_then(|p| p.parse().ok())
            .unwrap_or(8080);
        Self {
            host: env::var("IMTK_HOST").unwrap_or_else(|_| "0.0.0.0".to_string()),
            port,
            db_url: env::var("IMTK_DB_URL")
                .unwrap_or_else(|_| "sqlite://data/app.db?mode=rwc".to_string()),
            static_dir: env::var("IMTK_STATIC_DIR").ok().filter(|s| !s.is_empty()),
            cookie_secure: env::var("IMTK_COOKIE_SECURE")
                .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
                .unwrap_or(false),
            smtp_host: env::var("IMTK_SMTP_HOST").unwrap_or_default(),
            smtp_port: env::var("IMTK_SMTP_PORT")
                .ok()
                .and_then(|p| p.parse().ok())
                .unwrap_or(587),
            smtp_user: env::var("IMTK_SMTP_USER").unwrap_or_default(),
            smtp_pass: env::var("IMTK_SMTP_PASS").unwrap_or_default(),
            smtp_from: env::var("IMTK_SMTP_FROM")
                .unwrap_or_else(|_| "Atolye Takip <no-reply@localhost>".to_string()),
            app_url: env::var("IMTK_APP_URL")
                .unwrap_or_else(|_| "http://localhost:5173".to_string()),
        }
    }
}
