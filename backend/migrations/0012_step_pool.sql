-- 0012: Adim Havuzu — workspace genelinde yeniden kullanilabilir surec adimi tanimlari.
-- Adim bir kez tanimlanir (varsayilan sorumlu + onay kurali ile), surec gruplari
-- buradan secilerek kurulur. Grup = workflow_template; node'lar publish aninda
-- havuz degerlerinin anlik kopyasini alir (immutable versiyon korunur).

CREATE TABLE step_definitions (
    id                      TEXT PRIMARY KEY,
    workspace_id            TEXT NOT NULL REFERENCES workspaces(id),
    name                    TEXT NOT NULL,
    description             TEXT,
    default_assignee_type   TEXT,             -- user|team|NULL
    default_assignee_id     TEXT,
    requires_approval       INTEGER NOT NULL DEFAULT 0,
    approver_role_id        TEXT,             -- NULL = onay yetkisi olan herkes
    sort_order              INTEGER NOT NULL DEFAULT 0,
    archived_at             TEXT,
    created_at              TEXT NOT NULL,
    UNIQUE(workspace_id, name)
);

CREATE INDEX idx_step_defs_ws ON step_definitions(workspace_id, archived_at);
