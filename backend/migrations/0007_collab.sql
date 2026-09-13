-- Faz 8: Yorum + Dosya + Audit (yol haritasi 33-35)
-- Yorumlar polymorphic: section | work_item | process_instance
-- Attachment'ler diskte (data/uploads) saklanir, kayit DB'de.
-- audit_logs APPEND-ONLY: degistirme/silme endpoint'i YOKTUR.

CREATE TABLE comments (
    id           TEXT PRIMARY KEY,
    workspace_id TEXT NOT NULL REFERENCES workspaces(id),
    entity_type  TEXT NOT NULL, -- section | work_item | process_instance
    entity_id    TEXT NOT NULL,
    body         TEXT NOT NULL,
    created_by   TEXT NOT NULL REFERENCES users(id),
    created_at   TEXT NOT NULL,
    edited_at    TEXT
);
CREATE INDEX idx_comments_entity ON comments(entity_type, entity_id, created_at);

CREATE TABLE attachments (
    id           TEXT PRIMARY KEY,
    workspace_id TEXT NOT NULL REFERENCES workspaces(id),
    entity_type  TEXT NOT NULL, -- section | work_item | process_instance
    entity_id    TEXT NOT NULL,
    storage_key  TEXT NOT NULL, -- data/uploads/{wid}/{aid}_{name}
    file_name    TEXT NOT NULL,
    mime_type    TEXT NOT NULL,
    size         INTEGER NOT NULL,
    uploaded_by  TEXT NOT NULL REFERENCES users(id),
    created_at   TEXT NOT NULL,
    archived_at  TEXT
);
CREATE INDEX idx_attachments_entity ON attachments(entity_type, entity_id, archived_at);

CREATE TABLE audit_logs (
    id             TEXT PRIMARY KEY,
    workspace_id   TEXT NOT NULL REFERENCES workspaces(id),
    actor_user_id  TEXT REFERENCES users(id),
    action         TEXT NOT NULL, -- orn: process.approved, rework.started, workflow.published
    entity_type    TEXT,
    entity_id      TEXT,
    metadata_json  TEXT,
    created_at     TEXT NOT NULL
);
CREATE INDEX idx_audit_workspace ON audit_logs(workspace_id, created_at DESC);
CREATE INDEX idx_audit_entity ON audit_logs(entity_type, entity_id);
