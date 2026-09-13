use std::collections::HashSet;

use sqlx::SqlitePool;

use crate::auth::AuthUser;
use crate::error::{AppError, AppResult};

/// Bir istek baglaminda kullaniciyi bir workspace'e baglayan kontekst:
/// uyeligi, rolu, izinleri ve section kapsam filtresi.
pub struct WsCtx {
    pub member_id: String,
    pub role_name: String,
    pub is_owner: bool,
    pub permissions: HashSet<String>,
    /// None = tum workspace erisimi; Some = sadece listelenen subtree path'leri
    pub scope_paths: Option<Vec<String>>,
}

impl WsCtx {
    /// Kullanicinin workspace uyeligini ve yetki paketini yukler.
    pub async fn load(
        db: &SqlitePool,
        user: &AuthUser,
        workspace_id: &str,
    ) -> AppResult<Self> {
        let member: Option<(String, String, String, String, String)> = sqlx::query_as(
            "SELECT m.id, m.status, r.id, r.name, w.status
             FROM workspace_members m
             JOIN roles r ON r.id = m.role_id
             JOIN workspaces w ON w.id = m.workspace_id
             WHERE m.workspace_id = ?1 AND m.user_id = ?2 AND m.archived_at IS NULL",
        )
        .bind(workspace_id)
        .bind(&user.0.id)
        .fetch_optional(db)
        .await?;

        let (member_id, m_status, role_id, role_name, ws_status) = member
            .ok_or_else(|| AppError::Forbidden("Bu workspace'in uyesi degilsiniz".into()))?;
        if m_status != "active" {
            return Err(AppError::Forbidden("Uyeliginiz pasif durumda".into()));
        }
        if ws_status != "active" {
            return Err(AppError::Forbidden(
                "Workspace arsivlenmis, islem yapilamaz".into(),
            ));
        }

        let perms: Vec<(String,)> = sqlx::query_as(
            "SELECT p.key
             FROM role_permissions rp
             JOIN permissions p ON p.id = rp.permission_id
             WHERE rp.role_id = ?1",
        )
        .bind(&role_id)
        .fetch_all(db)
        .await?;
        let permissions: HashSet<String> = perms.into_iter().map(|(k,)| k).collect();

        let scopes: Vec<(String, String)> = sqlx::query_as(
            "SELECT s.id, s.path_cache
             FROM member_scopes ms
             JOIN sections s ON s.id = ms.section_id
             WHERE ms.member_id = ?1 AND s.archived_at IS NULL",
        )
        .bind(&member_id)
        .fetch_all(db)
        .await?;

        let (scope_paths, _scope_ids) = if scopes.is_empty() {
            (None, Vec::new())
        } else {
            let mut paths: Vec<String> = scopes.iter().map(|(_, p)| p.clone()).collect();
            let ids: Vec<String> = scopes.into_iter().map(|(i, _)| i).collect();
            paths.sort();
            paths.dedup();
            (Some(paths), ids)
        };

        Ok(Self {
            member_id,
            is_owner: role_name == "Owner",
            role_name,
            permissions,
            scope_paths,
        })
    }

    /// Izin kontrolu - yoksa 403.
    pub fn require(&self, perm: &str) -> AppResult<()> {
        if self.permissions.contains(perm) {
            Ok(())
        } else {
            Err(AppError::Forbidden("Bu islem icin yetkiniz yok".into()))
        }
    }

    /// Bir section (path_cache ile) bu kullanicinin kapsaminda mi?
    pub fn path_in_scope(&self, path: &str) -> bool {
        match &self.scope_paths {
            None => true,
            Some(paths) => paths
                .iter()
                .any(|sp| path == sp || path.starts_with(&format!("{sp}/"))),
        }
    }
}
