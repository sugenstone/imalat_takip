-- Ilk teknik sprint: Auth + Workspace + Yetki + Sections
-- Tum ID'ler UUIDv7 (TEXT), tarihler RFC3339 UTC TEXT.
-- PostgreSQL gecisinde: TEXT id -> UUID, tarihler -> TIMESTAMPTZ.

CREATE TABLE users (
    id            TEXT PRIMARY KEY,
    email         TEXT NOT NULL UNIQUE COLLATE NOCASE,
    password_hash TEXT NOT NULL,
    name          TEXT NOT NULL,
    is_active     INTEGER NOT NULL DEFAULT 1,
    email_verified_at TEXT, -- akis kapali, yapi hazir
    created_at    TEXT NOT NULL,
    updated_at    TEXT NOT NULL
);

CREATE TABLE sessions (
    id         TEXT PRIMARY KEY,
    user_id    TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    token_hash TEXT NOT NULL UNIQUE,
    user_agent TEXT,
    ip         TEXT,
    expires_at TEXT NOT NULL,
    created_at TEXT NOT NULL
);
CREATE INDEX idx_sessions_user    ON sessions(user_id);
CREATE INDEX idx_sessions_expires ON sessions(expires_at);

CREATE TABLE workspaces (
    id          TEXT PRIMARY KEY,
    name        TEXT NOT NULL,
    slug        TEXT NOT NULL UNIQUE,
    owner_id    TEXT NOT NULL REFERENCES users(id),
    status      TEXT NOT NULL DEFAULT 'active', -- active | archived
    created_at  TEXT NOT NULL,
    updated_at  TEXT NOT NULL,
    archived_at TEXT
);

CREATE TABLE permissions (
    id          INTEGER PRIMARY KEY,
    key         TEXT NOT NULL UNIQUE,
    description TEXT NOT NULL
);

CREATE TABLE roles (
    id          TEXT PRIMARY KEY,
    workspace_id TEXT NOT NULL REFERENCES workspaces(id),
    name        TEXT NOT NULL,
    description TEXT,
    is_system   INTEGER NOT NULL DEFAULT 0, -- Owner/Admin/Supervisor/Worker/Viewer
    created_at  TEXT NOT NULL,
    UNIQUE(workspace_id, name)
);

CREATE TABLE role_permissions (
    role_id       TEXT NOT NULL REFERENCES roles(id) ON DELETE CASCADE,
    permission_id INTEGER NOT NULL REFERENCES permissions(id),
    PRIMARY KEY (role_id, permission_id)
);

CREATE TABLE workspace_members (
    id           TEXT PRIMARY KEY,
    workspace_id TEXT NOT NULL REFERENCES workspaces(id),
    user_id      TEXT NOT NULL REFERENCES users(id),
    role_id      TEXT NOT NULL REFERENCES roles(id),
    status       TEXT NOT NULL DEFAULT 'active', -- active | removed
    joined_at    TEXT NOT NULL,
    created_at   TEXT NOT NULL,
    archived_at  TEXT,
    UNIQUE(workspace_id, user_id)
);
CREATE INDEX idx_members_user ON workspace_members(user_id);

-- Uye icin kayit yoksa = tum workspace erisimi.
-- Kayit varsa SADECE atanmis section subtree'lerine erisim.
CREATE TABLE member_scopes (
    id         TEXT PRIMARY KEY,
    member_id  TEXT NOT NULL REFERENCES workspace_members(id) ON DELETE CASCADE,
    section_id TEXT NOT NULL REFERENCES sections(id),
    created_at TEXT NOT NULL,
    UNIQUE(member_id, section_id)
);

CREATE TABLE teams (
    id           TEXT PRIMARY KEY,
    workspace_id TEXT NOT NULL REFERENCES workspaces(id),
    name         TEXT NOT NULL,
    description  TEXT,
    created_at   TEXT NOT NULL,
    updated_at   TEXT NOT NULL,
    archived_at  TEXT,
    UNIQUE(workspace_id, name)
);

CREATE TABLE team_members (
    team_id  TEXT NOT NULL REFERENCES teams(id) ON DELETE CASCADE,
    user_id  TEXT NOT NULL REFERENCES users(id),
    added_at TEXT NOT NULL,
    PRIMARY KEY (team_id, user_id)
);

CREATE TABLE sections (
    id           TEXT PRIMARY KEY,
    workspace_id TEXT NOT NULL REFERENCES workspaces(id),
    parent_id    TEXT REFERENCES sections(id),
    name         TEXT NOT NULL,
    sort_order   INTEGER NOT NULL DEFAULT 0,
    depth        INTEGER NOT NULL DEFAULT 0, -- kok = 0
    path_cache   TEXT NOT NULL, -- materialized path: '/rootId/childId/...'
    created_at   TEXT NOT NULL,
    updated_at   TEXT NOT NULL,
    archived_at  TEXT
);
CREATE INDEX idx_sections_workspace ON sections(workspace_id, archived_at);
CREATE INDEX idx_sections_parent    ON sections(parent_id);
CREATE INDEX idx_sections_path      ON sections(path_cache);

-- Sabit izin listesi (yol haritasi 7.2) - kod tarafinda da permissions.rs'de tanimli.
INSERT INTO permissions (key, description) VALUES
    ('workspace.update',      'Workspace bilgilerini guncelleme'),
    ('workspace.archive',     'Workspace arsivleme'),
    ('section.create',        'Bolum olusturma'),
    ('section.update',        'Bolum guncelleme'),
    ('section.clone',         'Bolum kopyalama'),
    ('section.archive',       'Bolum arsivleme'),
    ('section.bulk_create',   'Seri bolum olusturma'),
    ('work_item.create',      'Is kalemi olusturma'),
    ('work_item.update',      'Is kalemi guncelleme'),
    ('work_item.archive',     'Is kalemi arsivleme'),
    ('work_item.assign_workflow', 'Is kalemine workflow atama'),
    ('workflow.create',       'Workflow sablonu olusturma'),
    ('workflow.update',       'Workflow sablonu guncelleme'),
    ('workflow.publish',      'Workflow versiyonu yayinlama'),
    ('workflow.assign',       'Workflow atama'),
    ('process.start',         'Surec baslatma'),
    ('process.complete',      'Surec tamamlama'),
    ('process.fail',          'Surec basarisiz isaretleme'),
    ('process.rework',        'Rework baslatma'),
    ('process.approve',       'Surec onaylama'),
    ('process.assign',        'Surec atama'),
    ('team.manage',           'Takim yonetimi'),
    ('user.invite',           'Uye ekleme/cikarma'),
    ('role.manage',           'Rol ve kapsam yonetimi'),
    ('notification.manage',   'Bildirim yonetimi');
