use axum::extract::{Path, Query, State};
use axum::{routing, Json, Router};
use serde::{Deserialize, Serialize};

use crate::auth::AuthUser;
use crate::error::{AppError, AppResult};
use crate::state::AppState;
use crate::util;
use crate::ws_ctx::WsCtx;

#[derive(Serialize, sqlx::FromRow, Clone)]
pub struct SectionRow {
    pub id: String,
    pub parent_id: Option<String>,
    pub name: String,
    pub sort_order: i64,
    pub depth: i64,
    pub archived: bool,
    /// Alt agactaki is kalemi sayisi (ilerleme kartlari icin)
    pub item_count: i64,
    /// Alt agactaki AKTIF akislardaki surec toplami / onaylanan
    pub process_total: i64,
    pub process_approved: i64,
}

#[derive(Deserialize)]
pub struct CreateSectionReq {
    #[serde(default)]
    parent_id: Option<String>,
    name: String,
}

/// Seri olusturma: "Daire {n}" sablonu, start..end araligi.
#[derive(Deserialize)]
pub struct SerialCreateReq {
    #[serde(default)]
    parent_id: Option<String>,
    template: String,
    start: i64,
    end: i64,
}

#[derive(Deserialize)]
pub struct UpdateSectionReq {
    name: String,
}

#[derive(Deserialize)]
pub struct CloneReq {
    #[serde(default)]
    name: Option<String>,
}

#[derive(Deserialize)]
pub struct MoveReq {
    /// None = kok seviyesine tasi
    #[serde(default)]
    parent_id: Option<String>,
}

#[derive(Deserialize, Default)]
pub struct ListQuery {
    /// Arsivlenmisleri de goster
    #[serde(default)]
    include_archived: bool,
}

const MAX_SERIAL: i64 = 500;

async fn load_section(
    db: &sqlx::SqlitePool,
    wid: &str,
    sid: &str,
) -> AppResult<(String, Option<String>, String, String, i64)> {
    // (id, parent_id, name, path_cache, depth)
    let row: Option<(String, Option<String>, String, String, i64)> = sqlx::query_as(
        "SELECT id, parent_id, name, path_cache, depth FROM sections WHERE id = ?1 AND workspace_id = ?2",
    )
    .bind(sid)
    .bind(wid)
    .fetch_optional(db)
    .await?;
    row.ok_or_else(|| AppError::NotFound("Bolum bulunamadi".into()))
}

async fn next_sort(db: &sqlx::SqlitePool, wid: &str, parent_id: Option<&str>) -> AppResult<i64> {
    let max: Option<(i64,)> = match parent_id {
        Some(p) => sqlx::query_as(
            "SELECT COALESCE(MAX(sort_order), 0) FROM sections WHERE workspace_id = ?1 AND parent_id = ?2",
        )
        .bind(wid)
        .bind(p)
        .fetch_optional(db)
        .await?,
        None => sqlx::query_as(
            "SELECT COALESCE(MAX(sort_order), 0) FROM sections WHERE workspace_id = ?1 AND parent_id IS NULL",
        )
        .bind(wid)
        .fetch_optional(db)
        .await?,
    };
    Ok(max.map(|(v,)| v).unwrap_or(0) + 1)
}

/// Kapsam filtresi: scope'lu kullanici yalnizca atanmis subtree'leri gorur.

pub async fn list(
    State(state): State<AppState>,
    user: AuthUser,
    Path(wid): Path<String>,
    Query(q): Query<ListQuery>,
) -> AppResult<Json<Vec<SectionRow>>> {
    let ctx = WsCtx::load(&state.db, &user, &wid).await?;

    // Ilerleme: alt agac (subtree) bazli is kalemi + surec sayilari (korele subquery)
    const PROG: &str = ",
        (SELECT COUNT(*) FROM work_items w JOIN sections sub ON sub.id = w.section_id
          WHERE w.workspace_id = s.workspace_id AND w.archived_at IS NULL
            AND (sub.id = s.id OR sub.path_cache LIKE s.path_cache || '/%')) AS item_count,
        (SELECT COUNT(*) FROM process_instances pi
          JOIN workflow_instances wi ON wi.id = pi.workflow_instance_id
          JOIN work_items w2 ON w2.id = wi.work_item_id
          JOIN sections sub2 ON sub2.id = w2.section_id
          WHERE wi.status = 'active' AND w2.archived_at IS NULL
            AND (sub2.id = s.id OR sub2.path_cache LIKE s.path_cache || '/%')) AS process_total,
        (SELECT COUNT(*) FROM process_instances pi2
          JOIN workflow_instances wi2 ON wi2.id = pi2.workflow_instance_id
          JOIN work_items w3 ON w3.id = wi2.work_item_id
          JOIN sections sub3 ON sub3.id = w3.section_id
          WHERE wi2.status = 'active' AND w3.archived_at IS NULL AND pi2.status = 'approved'
            AND (sub3.id = s.id OR sub3.path_cache LIKE s.path_cache || '/%')) AS process_approved";
    let sql = if q.include_archived {
        format!("SELECT s.id, s.parent_id, s.name, s.sort_order, s.depth, s.archived_at IS NOT NULL AS archived{PROG}
         FROM sections s WHERE s.workspace_id = ?1
         ORDER BY s.depth, s.sort_order, s.name")
    } else {
        format!("SELECT s.id, s.parent_id, s.name, s.sort_order, s.depth, s.archived_at IS NOT NULL AS archived{PROG}
         FROM sections s WHERE s.workspace_id = ?1 AND s.archived_at IS NULL
         ORDER BY s.depth, s.sort_order, s.name")
    };
    let sql = sql.as_str();
    let rows = sqlx::query_as::<_, SectionRow>(sql)
        .bind(&wid)
        .fetch_all(&state.db)
        .await?;

    // Kapsam filtresi: scope subtree disindakileri cikar
    let rows = match &ctx.scope_paths {
        None => rows,
        Some(_) => {
            // path_cache ile filtreleme yapmak icin id->path gerekiyor; ayri sorgu yerine
            // scope section'larinin path'lerini WsCtx zaten tasiyor (scope_paths).
            // SectionRow'da path yok, bu yuzden id bazli gorunurluk hesaplanir:
            // gorunen = scope kokleri VE onlarin alt dugumleri. Alt iliskiyi kurmak icin
            // tum id->path bilgisini tekrar cekmek gerekir; en verimli yol tek sorguda cozmek.
            let paths = sqlx::query_as::<_, (String, String)>(
                "SELECT id, path_cache FROM sections WHERE workspace_id = ?1",
            )
            .bind(&wid)
            .fetch_all(&state.db)
            .await?;
            let id_to_path: std::collections::HashMap<String, String> =
                paths.into_iter().collect();
            let scope_paths = ctx.scope_paths.clone().unwrap_or_default();
            rows.into_iter()
                .filter(|r| {
                    id_to_path
                        .get(&r.id)
                        .map(|p| {
                            scope_paths
                                .iter()
                                .any(|sp| p == sp || p.starts_with(&format!("{sp}/")))
                        })
                        .unwrap_or(false)
                })
                .collect()
        }
    };
    Ok(Json(rows))
}

pub async fn create(
    State(state): State<AppState>,
    user: AuthUser,
    Path(wid): Path<String>,
    Json(req): Json<CreateSectionReq>,
) -> AppResult<(axum::http::StatusCode, Json<SectionRow>)> {
    let ctx = WsCtx::load(&state.db, &user, &wid).await?;
    ctx.require("section.create")?;

    let name = req.name.trim().to_string();
    if name.is_empty() || name.len() > 120 {
        return Err(AppError::BadRequest("Bolum adi 1-120 karakter olmali".into()));
    }

    let (parent_path, _parent_name, parent_depth) = match &req.parent_id {
        Some(pid) => {
            let (_, _, pname, ppath, pdepth) =
                load_section(&state.db, &wid, pid).await?;
            if !ctx.path_in_scope(&ppath) {
                return Err(AppError::Forbidden("Kapsam disindaki bolumun altina ekleme yapamazsiniz".into()));
            }
            (ppath, Some(pname), pdepth)
        }
        None => {
            if ctx.scope_paths.is_some() {
                return Err(AppError::Forbidden(
                    "Kapsaminiz kok seviyede bolum olusturmayi icermiyor".into(),
                ));
            }
            (String::new(), None, 0)
        }
    };

    let id = util::new_id();
    let now = util::now();
    let path = format!("{parent_path}/{id}");
    let sort = next_sort(&state.db, &wid, req.parent_id.as_deref()).await?;

    sqlx::query(
        "INSERT INTO sections (id, workspace_id, parent_id, name, sort_order, depth, path_cache, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8)",
    )
    .bind(&id)
    .bind(&wid)
    .bind(&req.parent_id)
    .bind(&name)
    .bind(sort)
    .bind(parent_depth + 1)
    .bind(&path)
    .bind(&now)
    .execute(&state.db)
    .await?;

    let row = SectionRow {
        id,
        parent_id: req.parent_id,
        name,
        sort_order: sort,
        depth: parent_depth + 1,
        archived: false,
        item_count: 0,
        process_total: 0,
        process_approved: 0,
    };
    Ok((axum::http::StatusCode::CREATED, Json(row)))
}

/// Seri bolum olusturma: "Daire {n}", 1..50 gibi. Tek transaction.
pub async fn create_serial(
    State(state): State<AppState>,
    user: AuthUser,
    Path(wid): Path<String>,
    Json(req): Json<SerialCreateReq>,
) -> AppResult<Json<serde_json::Value>> {
    let ctx = WsCtx::load(&state.db, &user, &wid).await?;
    ctx.require("section.bulk_create")?;

    if req.start < 0 || req.end < req.start || (req.end - req.start + 1) > MAX_SERIAL {
        return Err(AppError::BadRequest(format!(
            "Aralik gecersiz ya da cok buyuk (max {MAX_SERIAL} kayit)"
        )));
    }
    if !req.template.contains("{n") && !req.template.contains("{parent}") {
        return Err(AppError::BadRequest(
            "Sablon icinde {n}, {nn}, {nnn} veya {parent} kullanilmali".into(),
        ));
    }

    let (parent_path, parent_name, parent_depth) = match &req.parent_id {
        Some(pid) => {
            let (_, _, pname, ppath, pdepth) =
                load_section(&state.db, &wid, pid).await?;
            if !ctx.path_in_scope(&ppath) {
                return Err(AppError::Forbidden("Kapsam disi bolumun altina ekleme yapamazsiniz".into()));
            }
            (ppath, Some(pname), pdepth)
        }
        None => {
            if ctx.scope_paths.is_some() {
                return Err(AppError::Forbidden(
                    "Kapsaminiz kok seviyede bolum olusturmayi icermiyor".into(),
                ));
            }
            (String::new(), None, 0)
        }
    };

    // Dikkat: butun pool sorgulari tx acilmadan once yapilmali (tek baglanti kilidi).
    let now = util::now();
    let mut sort = next_sort(&state.db, &wid, req.parent_id.as_deref()).await?;

    let mut tx = state.db.begin().await?;
    let mut created = 0i64;

    for n in req.start..=req.end {
        let name = util::render_serial_name(&req.template, n as usize, parent_name.as_deref());
        if name.trim().is_empty() {
            continue;
        }
        let id = util::new_id();
        let path = format!("{parent_path}/{id}");
        sqlx::query(
            "INSERT INTO sections (id, workspace_id, parent_id, name, sort_order, depth, path_cache, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8)",
        )
        .bind(&id)
        .bind(&wid)
        .bind(&req.parent_id)
        .bind(&name)
        .bind(sort)
        .bind(parent_depth + 1)
        .bind(&path)
        .bind(&now)
        .execute(&mut *tx)
        .await?;
        sort += 1;
        created += 1;
    }
    tx.commit().await?;

    Ok(Json(serde_json::json!({ "created": created })))
}

// ---------------------------------------------------------------------------
// Ic ice seri olusturma (Kat x Daire) — gercek senaryo paketi
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct OuterSpec {
    template: String,
    start: i64,
    end: i64,
}

#[derive(Deserialize)]
pub struct InnerSpec {
    template: String,
    count: i64,
    /// continue: kat k'nin ogeleri (k-1)*count+1'den devam eder (Daire 1-10, 11-20...)
    /// restart: her dista seviyede bastan baslar (Daire 1-10 her katta)
    #[serde(default = "default_numbering")]
    numbering: String,
    /// restart modunda baslangic numarasi (default 1)
    #[serde(default = "one")]
    start: i64,
}

fn default_numbering() -> String {
    "continue".into()
}

fn one() -> i64 {
    1
}

#[derive(Deserialize)]
pub struct NestedSerialReq {
    #[serde(default)]
    parent_id: Option<String>,
    outer: OuterSpec,
    inner: InnerSpec,
}

/// Ic ice seri: "Kat 1-15" x her katta 10 "Daire {n}" — tek transaction.
/// continue modunda daire numarlari global olarak artar (1-10, 11-20, ..., 141-150).
pub async fn create_serial_nested(
    State(state): State<AppState>,
    user: AuthUser,
    Path(wid): Path<String>,
    Json(req): Json<NestedSerialReq>,
) -> AppResult<Json<serde_json::Value>> {
    let ctx = WsCtx::load(&state.db, &user, &wid).await?;
    ctx.require("section.bulk_create")?;

    let outer_count = req.outer.end - req.outer.start + 1;
    if req.outer.start < 0 || outer_count < 1 || req.inner.count < 1 {
        return Err(AppError::BadRequest("Gecersiz aralik/adet".into()));
    }
    if outer_count * req.inner.count > MAX_SERIAL {
        return Err(AppError::BadRequest(format!(
            "Toplam kayit cok buyuk (max {MAX_SERIAL}): {outer_count} x {}",
            req.inner.count
        )));
    }
    if !req.outer.template.contains("{n") && !req.outer.template.contains("{parent}") {
        return Err(AppError::BadRequest("Dis sablonda {n}/{nn}/{nnn}/{parent} kullanilmali".into()));
    }
    if !req.inner.template.contains("{n") && !req.inner.template.contains("{parent}") {
        return Err(AppError::BadRequest("Ic sablonda {n}/{nn}/{nnn}/{parent} kullanilmali".into()));
    }
    if !matches!(req.inner.numbering.as_str(), "continue" | "restart") {
        return Err(AppError::BadRequest("numbering continue|restart olmali".into()));
    }

    let (parent_path, parent_name, parent_depth) = match &req.parent_id {
        Some(pid) => {
            let (_, _, pname, ppath, pdepth) =
                load_section(&state.db, &wid, pid).await?;
            if !ctx.path_in_scope(&ppath) {
                return Err(AppError::Forbidden("Kapsam disi bolumun altina ekleme yapamazsiniz".into()));
            }
            (ppath, Some(pname), pdepth)
        }
        None => {
            if ctx.scope_paths.is_some() {
                return Err(AppError::Forbidden(
                    "Kapsaminiz kok seviyede bolum olusturmayi icermiyor".into(),
                ));
            }
            (String::new(), None, 0)
        }
    };

    let now = util::now();
    let mut tx = state.db.begin().await?;
    let outer_sort; // kok altinda siralama
    {
        // mevcut max sort (ayni seviyede)
        let max: Option<(i64,)> = match &req.parent_id {
            Some(p) => sqlx::query_as(
                "SELECT COALESCE(MAX(sort_order), 0) FROM sections WHERE workspace_id = ?1 AND parent_id = ?2",
            )
            .bind(&wid)
            .bind(p)
            .fetch_optional(&mut *tx)
            .await?,
            None => sqlx::query_as(
                "SELECT COALESCE(MAX(sort_order), 0) FROM sections WHERE workspace_id = ?1 AND parent_id IS NULL",
            )
            .bind(&wid)
            .fetch_optional(&mut *tx)
            .await?,
        };
        outer_sort = max.map(|(v,)| v).unwrap_or(0) + 1;
    }

    let mut created_outers = 0i64;
    let mut created_inners = 0i64;

    for (oi, on) in (req.outer.start..=req.outer.end).enumerate() {
        let outer_name = util::render_serial_name(&req.outer.template, on as usize, parent_name.as_deref());
        let outer_id = util::new_id();
        let outer_path = format!("{parent_path}/{outer_id}");

        sqlx::query(
            "INSERT INTO sections (id, workspace_id, parent_id, name, sort_order, depth, path_cache, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8)",
        )
        .bind(&outer_id)
        .bind(&wid)
        .bind(&req.parent_id)
        .bind(&outer_name)
        .bind(outer_sort + oi as i64)
        .bind(parent_depth + 1)
        .bind(&outer_path)
        .bind(&now)
        .execute(&mut *tx)
        .await?;
        created_outers += 1;

        // Ic numaralar: continue -> (oi)*count + 1'den; restart -> inner.start
        let inner_from = if req.inner.numbering == "continue" {
            oi as i64 * req.inner.count + req.inner.start
        } else {
            req.inner.start
        };

        for ii in 0..req.inner.count {
            let num = inner_from + ii;
            let inner_name = util::render_serial_name(
                &req.inner.template,
                num as usize,
                Some(&outer_name),
            );
            let inner_id = util::new_id();
            let inner_path = format!("{outer_path}/{inner_id}");
            sqlx::query(
                "INSERT INTO sections (id, workspace_id, parent_id, name, sort_order, depth, path_cache, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8)",
            )
            .bind(&inner_id)
            .bind(&wid)
            .bind(&outer_id)
            .bind(&inner_name)
            .bind(ii + 1)
            .bind(parent_depth + 2)
            .bind(&inner_path)
            .bind(&now)
            .execute(&mut *tx)
            .await?;
            created_inners += 1;
        }
    }
    tx.commit().await?;

    Ok(Json(serde_json::json!({
        "created": created_outers + created_inners,
        "outers": created_outers,
        "inners": created_inners
    })))
}

pub async fn update(
    State(state): State<AppState>,
    user: AuthUser,
    Path((wid, sid)): Path<(String, String)>,
    Json(req): Json<UpdateSectionReq>,
) -> AppResult<Json<SectionRow>> {
    let ctx = WsCtx::load(&state.db, &user, &wid).await?;
    ctx.require("section.update")?;

    let (_, parent_id, _, path, depth) = load_section(&state.db, &wid, &sid).await?;
    if !ctx.path_in_scope(&path) {
        return Err(AppError::Forbidden("Kapsam disi bolum".into()));
    }

    let name = req.name.trim().to_string();
    if name.is_empty() || name.len() > 120 {
        return Err(AppError::BadRequest("Bolum adi 1-120 karakter olmali".into()));
    }
    sqlx::query("UPDATE sections SET name = ?1, updated_at = ?2 WHERE id = ?3")
        .bind(&name)
        .bind(util::now())
        .bind(&sid)
        .execute(&state.db)
        .await?;

    let sort: (i64,) = sqlx::query_as("SELECT sort_order FROM sections WHERE id = ?1")
        .bind(&sid)
        .fetch_one(&state.db)
        .await?;
    Ok(Json(SectionRow {
        id: sid,
        parent_id,
        name,
        sort_order: sort.0,
        depth,
        archived: false,
        item_count: 0,
        process_total: 0,
        process_approved: 0,
    }))
}

/// Bolumu tum alt agaci ile arsivler (soft delete).
pub async fn archive(
    State(state): State<AppState>,
    user: AuthUser,
    Path((wid, sid)): Path<(String, String)>,
) -> AppResult<Json<serde_json::Value>> {
    let ctx = WsCtx::load(&state.db, &user, &wid).await?;
    ctx.require("section.archive")?;

    let (_, _, _, path, _) = load_section(&state.db, &wid, &sid).await?;
    if !ctx.path_in_scope(&path) {
        return Err(AppError::Forbidden("Kapsam disi bolum".into()));
    }

    let now = util::now();
    let res = sqlx::query(
        "UPDATE sections SET archived_at = ?1, updated_at = ?1
         WHERE workspace_id = ?2 AND path_cache LIKE ?3 || '%' AND archived_at IS NULL",
    )
    .bind(&now)
    .bind(&wid)
    .bind(&path)
    .execute(&state.db)
    .await?;
    Ok(Json(serde_json::json!({ "archived": res.rows_affected() })))
}

/// Bolumu alt agaci ile kopyalar. Ustten asliyla ayni seviyeye eklenir.
pub async fn clone(
    State(state): State<AppState>,
    user: AuthUser,
    Path((wid, sid)): Path<(String, String)>,
    Json(req): Json<CloneReq>,
) -> AppResult<(axum::http::StatusCode, Json<SectionRow>)> {
    let ctx = WsCtx::load(&state.db, &user, &wid).await?;
    ctx.require("section.clone")?;

    let (_, parent_id, src_name, src_path, _) = load_section(&state.db, &wid, &sid).await?;
    if !ctx.path_in_scope(&src_path) {
        return Err(AppError::Forbidden("Kapsam disi bolum".into()));
    }

    // Kaynak subtree'yi oku (parent-first siralamayi depth saglar)
    let subtree: Vec<(String, Option<String>, String, i64, String, i64)> = sqlx::query_as(
        "SELECT id, parent_id, name, sort_order, path_cache, depth
         FROM sections
         WHERE workspace_id = ?1 AND path_cache LIKE ?2 || '%'
         ORDER BY depth, sort_order",
    )
    .bind(&wid)
    .bind(&src_path)
    .fetch_all(&state.db)
    .await?;

    let new_root_name = req
        .name
        .map(|n| n.trim().to_string())
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| format!("{src_name} (Kopya)"));

    let new_root_id = util::new_id();
    let sort = next_sort(&state.db, &wid, parent_id.as_deref()).await?;

    // Kok node'un derinligi (kopya ayni seviyeye gider)
    let parent_depth = subtree.first().map(|s| s.5).unwrap_or(0);

    let now = util::now();
    let mut tx = state.db.begin().await?;
    let mut id_map: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();
    id_map.insert(sid.clone(), new_root_id.clone());

    for (i, (oid, oparent, oname, osort, _opath, odepth)) in subtree.iter().enumerate() {
        let nid = if i == 0 {
            new_root_id.clone()
        } else {
            util::new_id()
        };
        let nparent = oparent.as_ref().and_then(|p| id_map.get(p).cloned());
        let nname = if i == 0 { new_root_name.clone() } else { oname.clone() };
        // yeni path: parent'in yeni path'i + /nid - map'ten coz
        let npath = match &nparent {
            Some(p) => {
                let ppath: (String,) = sqlx::query_as(
                    "SELECT path_cache FROM sections WHERE id = ?1",
                )
                .bind(p)
                .fetch_one(&mut *tx)
                .await?;
                format!("{}/{}", ppath.0, nid)
            }
            None => format!("/{nid}"),
        };
        sqlx::query(
            "INSERT INTO sections (id, workspace_id, parent_id, name, sort_order, depth, path_cache, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8)",
        )
        .bind(&nid)
        .bind(&wid)
        .bind(&nparent)
        .bind(&nname)
        .bind(if i == 0 { sort } else { *osort })
        .bind(*odepth)
        .bind(&npath)
        .bind(&now)
        .execute(&mut *tx)
        .await?;
        id_map.insert(oid.clone(), nid);
    }
    tx.commit().await?;

    Ok((
        axum::http::StatusCode::CREATED,
        Json(SectionRow {
            id: new_root_id,
            parent_id,
            name: new_root_name,
            sort_order: sort,
            depth: parent_depth,
            archived: false,
            item_count: 0,
            process_total: 0,
            process_approved: 0,
        }),
    ))
}

/// Tasma: yeni parent altina (veya kok seviyesine). Cycle kontrolu ile.
pub async fn move_section(
    State(state): State<AppState>,
    user: AuthUser,
    Path((wid, sid)): Path<(String, String)>,
    Json(req): Json<MoveReq>,
) -> AppResult<Json<serde_json::Value>> {
    let ctx = WsCtx::load(&state.db, &user, &wid).await?;
    ctx.require("section.update")?;

    let (_, _, _, src_path, _) = load_section(&state.db, &wid, &sid).await?;
    if !ctx.path_in_scope(&src_path) {
        return Err(AppError::Forbidden("Kapsam disi bolum".into()));
    }
    if src_path.split('/').filter(|s| !s.is_empty()).count() == 0 {
        return Err(AppError::BadRequest("Gecersiz bolum".into()));
    }

    let (new_parent_path, new_parent_id, new_parent_depth) = match &req.parent_id {
        Some(pid) => {
            if pid == &sid {
                return Err(AppError::BadRequest("Bolum kendisinin altina tasinamaz".into()));
            }
            let (npid, npname, _, nppath, npdepth) = load_section(&state.db, &wid, pid).await?;
            let _ = npname;
            // cycle kontrolu: yeni parent, tasinan node'un alt agacinda OLAMAZ
            if nppath == src_path || nppath.starts_with(&format!("{src_path}/")) {
                return Err(AppError::BadRequest(
                    "Bolum, kendi alt agacindaki bir bolumun altina tasinamaz".into(),
                ));
            }
            if !ctx.path_in_scope(&nppath) {
                return Err(AppError::Forbidden("Kapsam disi hedef bolum".into()));
            }
            (nppath, Some(npid), npdepth)
        }
        None => {
            if ctx.scope_paths.is_some() {
                return Err(AppError::Forbidden(
                    "Kapsaminiz kok seviyeye tasimayi icermiyor".into(),
                ));
            }
            (String::new(), None, 0)
        }
    };

    let old_prefix = src_path.clone();
    let new_prefix = format!("{new_parent_path}/{sid}");
    // Eski depth: path'teki segment sayisi - 1 ('/a' -> depth 0). Yeni depth: new_parent_depth + 1.
    let old_depth = src_path.split('/').filter(|s| !s.is_empty()).count() as i64 - 1;
    let depth_delta = new_parent_depth + 1 - old_depth;

    let sort = next_sort(&state.db, &wid, new_parent_id.as_deref()).await?;
    let now = util::now();

    let mut tx = state.db.begin().await?;
    sqlx::query(
        "UPDATE sections SET parent_id = ?1, sort_order = ?2, updated_at = ?3 WHERE id = ?4",
    )
    .bind(&new_parent_id)
    .bind(sort)
    .bind(&now)
    .bind(&sid)
    .execute(&mut *tx)
    .await?;

    sqlx::query(
        "UPDATE sections
         SET path_cache = ?1 || substr(path_cache, length(?2) + 1),
             depth = depth + ?3,
             updated_at = ?4
         WHERE workspace_id = ?5 AND path_cache LIKE ?2 || '%'",
    )
    .bind(&new_prefix)
    .bind(&old_prefix)
    .bind(depth_delta)
    .bind(&now)
    .bind(&wid)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;

    Ok(Json(serde_json::json!({ "ok": true })))
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/workspaces/{wid}/sections", routing::get(list).post(create))
        .route("/workspaces/{wid}/sections/serial", routing::post(create_serial))
        .route("/workspaces/{wid}/sections/serial-nested", routing::post(create_serial_nested))
        .route(
            "/workspaces/{wid}/sections/{sid}",
            routing::patch(update).delete(axum::routing::any(|| async {
                AppError::BadRequest("Bolum silinemez, arsivleyin".to_string())
            })),
        )
        .route("/workspaces/{wid}/sections/{sid}/archive", routing::post(archive))
        .route("/workspaces/{wid}/sections/{sid}/clone", routing::post(clone))
        .route("/workspaces/{wid}/sections/{sid}/move", routing::post(move_section))
}
